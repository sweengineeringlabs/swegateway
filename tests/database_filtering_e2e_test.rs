//! End-to-end tests for MemoryDatabase sorting and filtering improvements (BL-004).
//!
//! Covers type-aware numeric sorting, null handling in sort order,
//! comparison operator filters (__gt, __lt, __gte, __lte, __like, __in),
//! and combined filter scenarios.

use swe_gateway::prelude::*;
use swe_gateway::saf;
use swe_gateway::saf::database::QueryParams;

fn make_product(
    id: &str,
    name: &str,
    price: impl Into<serde_json::Value>,
) -> serde_json::Map<String, serde_json::Value> {
    let mut r = serde_json::Map::new();
    r.insert("id".into(), serde_json::json!(id));
    r.insert("name".into(), serde_json::json!(name));
    r.insert("price".into(), price.into());
    r
}

fn make_product_with_category(
    id: &str,
    name: &str,
    price: impl Into<serde_json::Value>,
    category: &str,
) -> serde_json::Map<String, serde_json::Value> {
    let mut r = make_product(id, name, price);
    r.insert("category".into(), serde_json::json!(category));
    r
}

// =============================================================================
// Numeric sorting
// =============================================================================

#[tokio::test]
async fn test_query_order_by_numeric_field_sorts_numerically_not_lexicographically() {
    let db = saf::memory_database();

    // Insert in non-sorted order: 20, 1, 10, 2
    // Lexicographic sort would give: "1", "10", "2", "20"
    // Numeric sort should give: 1, 2, 10, 20
    db.insert("products", make_product("a", "Item20", serde_json::json!(20)))
        .await
        .unwrap();
    db.insert("products", make_product("b", "Item1", serde_json::json!(1)))
        .await
        .unwrap();
    db.insert("products", make_product("c", "Item10", serde_json::json!(10)))
        .await
        .unwrap();
    db.insert("products", make_product("d", "Item2", serde_json::json!(2)))
        .await
        .unwrap();

    let results = db
        .query("products", QueryParams::new().order_by("price", false))
        .await
        .unwrap();

    let prices: Vec<f64> = results
        .iter()
        .map(|r| r.get("price").unwrap().as_f64().unwrap())
        .collect();

    assert_eq!(prices, vec![1.0, 2.0, 10.0, 20.0], "should sort numerically ascending");
}

#[tokio::test]
async fn test_query_order_by_numeric_field_desc_sorts_numerically_descending() {
    let db = saf::memory_database();

    db.insert("products", make_product("a", "Cheap", serde_json::json!(5)))
        .await
        .unwrap();
    db.insert("products", make_product("b", "Expensive", serde_json::json!(100)))
        .await
        .unwrap();
    db.insert("products", make_product("c", "Mid", serde_json::json!(50)))
        .await
        .unwrap();

    let results = db
        .query("products", QueryParams::new().order_by("price", true))
        .await
        .unwrap();

    let prices: Vec<f64> = results
        .iter()
        .map(|r| r.get("price").unwrap().as_f64().unwrap())
        .collect();

    assert_eq!(prices, vec![100.0, 50.0, 5.0], "should sort numerically descending");
}

// =============================================================================
// Null handling in sort
// =============================================================================

#[tokio::test]
async fn test_query_order_by_nulls_sort_last_ascending() {
    let db = saf::memory_database();

    db.insert("products", make_product("a", "HasPrice", serde_json::json!(10)))
        .await
        .unwrap();

    // Record with null price
    let mut null_record = serde_json::Map::new();
    null_record.insert("id".into(), serde_json::json!("b"));
    null_record.insert("name".into(), serde_json::json!("NullPrice"));
    null_record.insert("price".into(), serde_json::Value::Null);
    db.insert("products", null_record).await.unwrap();

    db.insert("products", make_product("c", "AnotherPrice", serde_json::json!(5)))
        .await
        .unwrap();

    // Record with missing price field entirely
    let mut no_field = serde_json::Map::new();
    no_field.insert("id".into(), serde_json::json!("d"));
    no_field.insert("name".into(), serde_json::json!("MissingPrice"));
    db.insert("products", no_field).await.unwrap();

    let results = db
        .query("products", QueryParams::new().order_by("price", false))
        .await
        .unwrap();

    let names: Vec<&str> = results
        .iter()
        .map(|r| r.get("name").unwrap().as_str().unwrap())
        .collect();

    // Non-null values first (sorted), then nulls/missing last
    assert_eq!(names[0], "AnotherPrice", "smallest price first");
    assert_eq!(names[1], "HasPrice", "second smallest price second");
    // The last two should be null/missing (order between them is not guaranteed)
    let tail: Vec<&str> = names[2..].to_vec();
    assert!(
        tail.contains(&"NullPrice") && tail.contains(&"MissingPrice"),
        "null and missing price records should sort last, got: {:?}",
        tail
    );
}

#[tokio::test]
async fn test_query_order_by_nulls_sort_last_descending() {
    let db = saf::memory_database();

    db.insert("products", make_product("a", "High", serde_json::json!(100)))
        .await
        .unwrap();

    let mut null_record = serde_json::Map::new();
    null_record.insert("id".into(), serde_json::json!("b"));
    null_record.insert("name".into(), serde_json::json!("NullPrice"));
    null_record.insert("price".into(), serde_json::Value::Null);
    db.insert("products", null_record).await.unwrap();

    db.insert("products", make_product("c", "Low", serde_json::json!(5)))
        .await
        .unwrap();

    let results = db
        .query("products", QueryParams::new().order_by("price", true))
        .await
        .unwrap();

    let names: Vec<&str> = results
        .iter()
        .map(|r| r.get("name").unwrap().as_str().unwrap())
        .collect();

    // Descending: highest first, nulls still last
    assert_eq!(names[0], "High");
    assert_eq!(names[1], "Low");
    assert_eq!(names[2], "NullPrice");
}

// =============================================================================
// Greater than / less than filtering on numbers
// =============================================================================

#[tokio::test]
async fn test_filter_gt_returns_records_with_value_strictly_greater() {
    let db = saf::memory_database();

    for price in [10, 20, 30, 40, 50] {
        db.insert(
            "products",
            make_product(&price.to_string(), &format!("Item{}", price), serde_json::json!(price)),
        )
        .await
        .unwrap();
    }

    let results = db
        .query("products", QueryParams::new().filter("price__gt", serde_json::json!(30)))
        .await
        .unwrap();

    let prices: Vec<i64> = results
        .iter()
        .map(|r| r.get("price").unwrap().as_i64().unwrap())
        .collect();

    assert_eq!(prices.len(), 2, "should have 2 items with price > 30");
    assert!(prices.contains(&40));
    assert!(prices.contains(&50));
    assert!(!prices.contains(&30), "30 should not be included (strict gt)");
}

#[tokio::test]
async fn test_filter_lt_returns_records_with_value_strictly_less() {
    let db = saf::memory_database();

    for price in [10, 20, 30, 40, 50] {
        db.insert(
            "products",
            make_product(&price.to_string(), &format!("Item{}", price), serde_json::json!(price)),
        )
        .await
        .unwrap();
    }

    let results = db
        .query("products", QueryParams::new().filter("price__lt", serde_json::json!(30)))
        .await
        .unwrap();

    let prices: Vec<i64> = results
        .iter()
        .map(|r| r.get("price").unwrap().as_i64().unwrap())
        .collect();

    assert_eq!(prices.len(), 2, "should have 2 items with price < 30");
    assert!(prices.contains(&10));
    assert!(prices.contains(&20));
    assert!(!prices.contains(&30), "30 should not be included (strict lt)");
}

#[tokio::test]
async fn test_filter_gte_includes_boundary_value() {
    let db = saf::memory_database();

    for price in [10, 20, 30, 40, 50] {
        db.insert(
            "products",
            make_product(&price.to_string(), &format!("Item{}", price), serde_json::json!(price)),
        )
        .await
        .unwrap();
    }

    let results = db
        .query("products", QueryParams::new().filter("price__gte", serde_json::json!(30)))
        .await
        .unwrap();

    let prices: Vec<i64> = results
        .iter()
        .map(|r| r.get("price").unwrap().as_i64().unwrap())
        .collect();

    assert_eq!(prices.len(), 3, "should have 3 items with price >= 30");
    assert!(prices.contains(&30), "boundary value 30 must be included");
    assert!(prices.contains(&40));
    assert!(prices.contains(&50));
}

#[tokio::test]
async fn test_filter_lte_includes_boundary_value() {
    let db = saf::memory_database();

    for price in [10, 20, 30, 40, 50] {
        db.insert(
            "products",
            make_product(&price.to_string(), &format!("Item{}", price), serde_json::json!(price)),
        )
        .await
        .unwrap();
    }

    let results = db
        .query("products", QueryParams::new().filter("price__lte", serde_json::json!(30)))
        .await
        .unwrap();

    let prices: Vec<i64> = results
        .iter()
        .map(|r| r.get("price").unwrap().as_i64().unwrap())
        .collect();

    assert_eq!(prices.len(), 3, "should have 3 items with price <= 30");
    assert!(prices.contains(&10));
    assert!(prices.contains(&20));
    assert!(prices.contains(&30), "boundary value 30 must be included");
}

// =============================================================================
// Like filtering on strings
// =============================================================================

#[tokio::test]
async fn test_filter_like_matches_substring_case_insensitive() {
    let db = saf::memory_database();

    db.insert("products", make_product("1", "Red Widget", serde_json::json!(10)))
        .await
        .unwrap();
    db.insert("products", make_product("2", "Blue Gadget", serde_json::json!(20)))
        .await
        .unwrap();
    db.insert("products", make_product("3", "Green Widget Pro", serde_json::json!(30)))
        .await
        .unwrap();
    db.insert("products", make_product("4", "Yellow Gizmo", serde_json::json!(40)))
        .await
        .unwrap();

    let results = db
        .query(
            "products",
            QueryParams::new().filter("name__like", serde_json::json!("widget")),
        )
        .await
        .unwrap();

    assert_eq!(results.len(), 2, "should match 'Red Widget' and 'Green Widget Pro'");
    let names: Vec<&str> = results
        .iter()
        .map(|r| r.get("name").unwrap().as_str().unwrap())
        .collect();
    assert!(names.contains(&"Red Widget"));
    assert!(names.contains(&"Green Widget Pro"));
}

#[tokio::test]
async fn test_filter_like_no_match_returns_empty() {
    let db = saf::memory_database();

    db.insert("products", make_product("1", "Alpha", serde_json::json!(10)))
        .await
        .unwrap();

    let results = db
        .query(
            "products",
            QueryParams::new().filter("name__like", serde_json::json!("zzz")),
        )
        .await
        .unwrap();

    assert_eq!(results.len(), 0, "no records should match substring 'zzz'");
}

// =============================================================================
// In-list filtering
// =============================================================================

#[tokio::test]
async fn test_filter_in_matches_values_in_list() {
    let db = saf::memory_database();

    for cat in ["electronics", "books", "clothing", "food", "toys"] {
        db.insert(
            "products",
            make_product_with_category(cat, &format!("A {}", cat), serde_json::json!(10), cat),
        )
        .await
        .unwrap();
    }

    let results = db
        .query(
            "products",
            QueryParams::new().filter(
                "category__in",
                serde_json::json!(["electronics", "books", "toys"]),
            ),
        )
        .await
        .unwrap();

    assert_eq!(results.len(), 3, "should match 3 categories from the in-list");
    let categories: Vec<&str> = results
        .iter()
        .map(|r| r.get("category").unwrap().as_str().unwrap())
        .collect();
    assert!(categories.contains(&"electronics"));
    assert!(categories.contains(&"books"));
    assert!(categories.contains(&"toys"));
    assert!(!categories.contains(&"clothing"));
    assert!(!categories.contains(&"food"));
}

#[tokio::test]
async fn test_filter_in_with_numeric_values() {
    let db = saf::memory_database();

    for price in [10, 20, 30, 40, 50] {
        db.insert(
            "products",
            make_product(&price.to_string(), &format!("P{}", price), serde_json::json!(price)),
        )
        .await
        .unwrap();
    }

    let results = db
        .query(
            "products",
            QueryParams::new().filter("price__in", serde_json::json!([20, 40])),
        )
        .await
        .unwrap();

    let prices: Vec<i64> = results
        .iter()
        .map(|r| r.get("price").unwrap().as_i64().unwrap())
        .collect();

    assert_eq!(prices.len(), 2);
    assert!(prices.contains(&20));
    assert!(prices.contains(&40));
}

// =============================================================================
// Combined filters
// =============================================================================

#[tokio::test]
async fn test_combined_equality_and_comparison_filters() {
    let db = saf::memory_database();

    db.insert(
        "products",
        make_product_with_category("1", "Cheap Widget", serde_json::json!(5), "widgets"),
    )
    .await
    .unwrap();
    db.insert(
        "products",
        make_product_with_category("2", "Pricey Widget", serde_json::json!(50), "widgets"),
    )
    .await
    .unwrap();
    db.insert(
        "products",
        make_product_with_category("3", "Cheap Gadget", serde_json::json!(5), "gadgets"),
    )
    .await
    .unwrap();
    db.insert(
        "products",
        make_product_with_category("4", "Pricey Gadget", serde_json::json!(50), "gadgets"),
    )
    .await
    .unwrap();

    // Combine equality filter (category == widgets) with comparison (price > 10)
    let results = db
        .query(
            "products",
            QueryParams::new()
                .filter("category", "widgets")
                .filter("price__gt", serde_json::json!(10)),
        )
        .await
        .unwrap();

    assert_eq!(results.len(), 1, "only one widget with price > 10");
    assert_eq!(results[0].get("name").unwrap(), "Pricey Widget");
}

#[tokio::test]
async fn test_combined_range_filter_gte_and_lte() {
    let db = saf::memory_database();

    for price in [5, 15, 25, 35, 45] {
        db.insert(
            "products",
            make_product(&price.to_string(), &format!("P{}", price), serde_json::json!(price)),
        )
        .await
        .unwrap();
    }

    // Range: 15 <= price <= 35
    let results = db
        .query(
            "products",
            QueryParams::new()
                .filter("price__gte", serde_json::json!(15))
                .filter("price__lte", serde_json::json!(35)),
        )
        .await
        .unwrap();

    let prices: Vec<i64> = results
        .iter()
        .map(|r| r.get("price").unwrap().as_i64().unwrap())
        .collect();

    assert_eq!(prices.len(), 3, "should have 3 items in range [15, 35]");
    assert!(prices.contains(&15));
    assert!(prices.contains(&25));
    assert!(prices.contains(&35));
}

#[tokio::test]
async fn test_combined_like_and_in_filters() {
    let db = saf::memory_database();

    db.insert(
        "products",
        make_product_with_category("1", "Red Widget", serde_json::json!(10), "widgets"),
    )
    .await
    .unwrap();
    db.insert(
        "products",
        make_product_with_category("2", "Blue Widget", serde_json::json!(20), "widgets"),
    )
    .await
    .unwrap();
    db.insert(
        "products",
        make_product_with_category("3", "Red Gadget", serde_json::json!(30), "gadgets"),
    )
    .await
    .unwrap();
    db.insert(
        "products",
        make_product_with_category("4", "Blue Gadget", serde_json::json!(40), "gadgets"),
    )
    .await
    .unwrap();

    // Like "red" AND category in ["widgets", "gadgets"]
    let results = db
        .query(
            "products",
            QueryParams::new()
                .filter("name__like", serde_json::json!("Red"))
                .filter("category__in", serde_json::json!(["widgets", "gadgets"])),
        )
        .await
        .unwrap();

    assert_eq!(results.len(), 2, "should match Red Widget and Red Gadget");
    let names: Vec<&str> = results
        .iter()
        .map(|r| r.get("name").unwrap().as_str().unwrap())
        .collect();
    assert!(names.contains(&"Red Widget"));
    assert!(names.contains(&"Red Gadget"));
}

// =============================================================================
// Backward compatibility: existing equality filters still work
// =============================================================================

#[tokio::test]
async fn test_equality_filter_backward_compatible() {
    let db = saf::memory_database();

    db.insert(
        "products",
        make_product_with_category("1", "Widget", serde_json::json!(10), "widgets"),
    )
    .await
    .unwrap();
    db.insert(
        "products",
        make_product_with_category("2", "Gadget", serde_json::json!(20), "gadgets"),
    )
    .await
    .unwrap();

    // Standard equality filter (no operator suffix) must still work
    let results = db
        .query(
            "products",
            QueryParams::new().filter("category", "widgets"),
        )
        .await
        .unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].get("name").unwrap(), "Widget");
}

#[tokio::test]
async fn test_count_with_comparison_filters() {
    let db = saf::memory_database();

    for price in [10, 20, 30, 40, 50] {
        db.insert(
            "products",
            make_product(&price.to_string(), &format!("P{}", price), serde_json::json!(price)),
        )
        .await
        .unwrap();
    }

    let count = db
        .count(
            "products",
            QueryParams::new().filter("price__gte", serde_json::json!(30)),
        )
        .await
        .unwrap();

    assert_eq!(count, 3, "count should respect comparison filters");
}

// =============================================================================
// String sorting stays correct
// =============================================================================

#[tokio::test]
async fn test_query_order_by_string_field_sorts_alphabetically() {
    let db = saf::memory_database();

    db.insert("products", make_product("1", "Cherry", serde_json::json!(1)))
        .await
        .unwrap();
    db.insert("products", make_product("2", "Apple", serde_json::json!(2)))
        .await
        .unwrap();
    db.insert("products", make_product("3", "Banana", serde_json::json!(3)))
        .await
        .unwrap();

    let results = db
        .query("products", QueryParams::new().order_by("name", false))
        .await
        .unwrap();

    let names: Vec<&str> = results
        .iter()
        .map(|r| r.get("name").unwrap().as_str().unwrap())
        .collect();

    assert_eq!(names, vec!["Apple", "Banana", "Cherry"]);
}
