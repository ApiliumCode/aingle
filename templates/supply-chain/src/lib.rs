//! Supply Chain Zome Template
//!
//! A template for supply chain tracking on AIngle.
//! Provides full provenance tracking with IoT integration.
//!
//! ## Use Cases
//! - Product tracking
//! - Cold chain monitoring
//! - Authenticity verification
//! - Regulatory compliance
//!
//! ## Usage
//! ```bash
//! cp -r templates/supply-chain my-supply-chain
//! cargo build --target wasm32-unknown-unknown
//! ```

use adk::prelude::*;
use serde::{Deserialize, Serialize};

// ============================================================================
// Entry Types
// ============================================================================

/// A product in the supply chain
#[hdk_entry_helper]
#[derive(Clone)]
pub struct Product {
    /// Unique product identifier (SKU, serial, etc.)
    pub product_id: String,

    /// Product name
    pub name: String,

    /// Product category
    pub category: String,

    /// Manufacturing details
    pub manufacturer: Manufacturer,

    /// Product attributes
    pub attributes: serde_json::Value,

    /// Creation timestamp
    pub created_at: u64,
}

/// Manufacturer information
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Manufacturer {
    /// Company name
    pub name: String,

    /// Location
    pub location: String,

    /// Certification IDs
    pub certifications: Vec<String>,
}

/// A location/checkpoint in the supply chain
#[hdk_entry_helper]
#[derive(Clone)]
pub struct Location {
    /// Unique location ID
    pub location_id: String,

    /// Human-readable name
    pub name: String,

    /// Location type
    pub location_type: LocationType,

    /// GPS coordinates (optional)
    pub coordinates: Option<Coordinates>,

    /// Contact info
    pub contact: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum LocationType {
    Factory,
    Warehouse,
    DistributionCenter,
    Port,
    Customs,
    Retailer,
    Customer,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Coordinates {
    pub lat: f64,
    pub lng: f64,
}

/// A custody transfer event
#[hdk_entry_helper]
#[derive(Clone)]
pub struct CustodyEvent {
    /// Product being transferred
    pub product_id: String,

    /// Event timestamp
    pub timestamp: u64,

    /// Event type
    pub event_type: CustodyEventType,

    /// From location
    pub from_location: Option<String>,

    /// To location
    pub to_location: String,

    /// Handler/carrier
    pub handler: String,

    /// Environmental conditions at transfer
    pub conditions: Option<EnvironmentConditions>,

    /// Digital signature of handler
    pub signature: Option<String>,

    /// Additional metadata
    pub metadata: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum CustodyEventType {
    Created,
    Shipped,
    Received,
    Inspected,
    Stored,
    Delivered,
    Returned,
    Disposed,
}

/// Environmental conditions (from IoT sensors)
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EnvironmentConditions {
    /// Temperature in Celsius
    pub temperature: Option<f64>,

    /// Humidity percentage
    pub humidity: Option<f64>,

    /// Light exposure (lux)
    pub light: Option<f64>,

    /// Shock/vibration detected
    pub shock: Option<bool>,

    /// Sensor ID that recorded this
    pub sensor_id: Option<String>,
}

/// Quality inspection record
#[hdk_entry_helper]
#[derive(Clone)]
pub struct InspectionRecord {
    /// Product being inspected
    pub product_id: String,

    /// Inspection timestamp
    pub timestamp: u64,

    /// Inspector identifier
    pub inspector: String,

    /// Location of inspection
    pub location_id: String,

    /// Inspection result
    pub result: InspectionResult,

    /// Detailed findings
    pub findings: Vec<InspectionFinding>,

    /// Attachments (hashes of images/documents)
    pub attachments: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum InspectionResult {
    Passed,
    PassedWithNotes,
    Failed,
    RequiresReview,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct InspectionFinding {
    pub category: String,
    pub description: String,
    pub severity: FindingSeverity,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum FindingSeverity {
    Info,
    Minor,
    Major,
    Critical,
}

// ============================================================================
// Entry Definitions
// ============================================================================

#[hdk_entry_defs]
#[unit_enum(UnitEntryTypes)]
pub enum EntryTypes {
    #[entry_def(visibility = "public")]
    Product(Product),

    #[entry_def(visibility = "public")]
    Location(Location),

    #[entry_def(visibility = "public")]
    CustodyEvent(CustodyEvent),

    #[entry_def(visibility = "public")]
    InspectionRecord(InspectionRecord),
}

#[hdk_link_types]
pub enum LinkTypes {
    /// Product -> Custody events
    ProductToCustody,

    /// Product -> Inspections
    ProductToInspection,

    /// Location -> Products currently there
    LocationToProducts,

    /// All products anchor
    AllProducts,

    /// All locations anchor
    AllLocations,
}

// ============================================================================
// Zome Functions
// ============================================================================

/// Register a new product
#[hdk_extern]
pub fn register_product(product: Product) -> ExternResult<ActionHash> {
    let action_hash = create_entry(EntryTypes::Product(product.clone()))?;

    // Link to all products anchor
    let anchor = anchor_hash("all_products")?;
    create_link(
        anchor,
        action_hash.clone(),
        LinkTypes::AllProducts,
        product.product_id.as_bytes().to_vec(),
    )?;

    Ok(action_hash)
}

/// Register a new location
#[hdk_extern]
pub fn register_location(location: Location) -> ExternResult<ActionHash> {
    let action_hash = create_entry(EntryTypes::Location(location.clone()))?;

    // Link to all locations anchor
    let anchor = anchor_hash("all_locations")?;
    create_link(
        anchor,
        action_hash.clone(),
        LinkTypes::AllLocations,
        location.location_id.as_bytes().to_vec(),
    )?;

    Ok(action_hash)
}

/// Record a custody transfer event
#[hdk_extern]
pub fn record_custody_event(event: CustodyEvent) -> ExternResult<ActionHash> {
    let action_hash = create_entry(EntryTypes::CustodyEvent(event.clone()))?;

    // Link to product
    if let Some(product_hash) = get_product_hash(&event.product_id)? {
        create_link(
            product_hash.clone(),
            action_hash.clone(),
            LinkTypes::ProductToCustody,
            event.timestamp.to_be_bytes().to_vec(),
        )?;
    }

    // Update location links
    if let Some(from_loc) = &event.from_location {
        if let Some(loc_hash) = get_location_hash(from_loc)? {
            // Remove from previous location
            let links = get_links(loc_hash.clone(), LinkTypes::LocationToProducts, None)?;
            for link in links {
                if String::from_utf8(link.tag.0.clone()).ok() == Some(event.product_id.clone()) {
                    delete_link(link.create_link_hash)?;
                }
            }
        }
    }

    if let Some(loc_hash) = get_location_hash(&event.to_location)? {
        if let Some(product_hash) = get_product_hash(&event.product_id)? {
            create_link(
                loc_hash,
                product_hash,
                LinkTypes::LocationToProducts,
                event.product_id.as_bytes().to_vec(),
            )?;
        }
    }

    Ok(action_hash)
}

/// Record an inspection
#[hdk_extern]
pub fn record_inspection(record: InspectionRecord) -> ExternResult<ActionHash> {
    let action_hash = create_entry(EntryTypes::InspectionRecord(record.clone()))?;

    // Link to product
    if let Some(product_hash) = get_product_hash(&record.product_id)? {
        create_link(
            product_hash,
            action_hash.clone(),
            LinkTypes::ProductToInspection,
            record.timestamp.to_be_bytes().to_vec(),
        )?;
    }

    Ok(action_hash)
}

/// Get full provenance history for a product
#[hdk_extern]
pub fn get_product_history(product_id: String) -> ExternResult<ProductHistory> {
    let product_hash = get_product_hash(&product_id)?
        .ok_or(wasm_error!(WasmErrorInner::Guest("Product not found".into())))?;

    // Get product details
    let product = get(product_hash.clone(), GetOptions::default())?
        .and_then(|r| r.entry().to_app_option::<Product>().ok().flatten())
        .ok_or(wasm_error!(WasmErrorInner::Guest("Product not found".into())))?;

    // Get custody events
    let custody_links = get_links(product_hash.clone(), LinkTypes::ProductToCustody, None)?;
    let mut custody_events = Vec::new();
    for link in custody_links {
        if let Some(hash) = link.target.into_action_hash() {
            if let Some(record) = get(hash, GetOptions::default())? {
                if let Some(event) = record.entry().to_app_option::<CustodyEvent>()? {
                    custody_events.push(event);
                }
            }
        }
    }
    custody_events.sort_by_key(|e| e.timestamp);

    // Get inspections
    let inspection_links = get_links(product_hash, LinkTypes::ProductToInspection, None)?;
    let mut inspections = Vec::new();
    for link in inspection_links {
        if let Some(hash) = link.target.into_action_hash() {
            if let Some(record) = get(hash, GetOptions::default())? {
                if let Some(inspection) = record.entry().to_app_option::<InspectionRecord>()? {
                    inspections.push(inspection);
                }
            }
        }
    }
    inspections.sort_by_key(|i| i.timestamp);

    Ok(ProductHistory {
        product,
        custody_events,
        inspections,
    })
}

/// Get products at a location
#[hdk_extern]
pub fn get_products_at_location(location_id: String) -> ExternResult<Vec<Product>> {
    let location_hash = get_location_hash(&location_id)?
        .ok_or(wasm_error!(WasmErrorInner::Guest("Location not found".into())))?;

    let links = get_links(location_hash, LinkTypes::LocationToProducts, None)?;

    let mut products = Vec::new();
    for link in links {
        if let Some(hash) = link.target.into_action_hash() {
            if let Some(record) = get(hash, GetOptions::default())? {
                if let Some(product) = record.entry().to_app_option::<Product>()? {
                    products.push(product);
                }
            }
        }
    }

    Ok(products)
}

/// Verify product authenticity by checking provenance chain
#[hdk_extern]
pub fn verify_authenticity(product_id: String) -> ExternResult<AuthenticityResult> {
    let history = get_product_history(product_id)?;

    let mut issues = Vec::new();

    // Check 1: Product exists
    if history.custody_events.is_empty() {
        issues.push("No custody events recorded".to_string());
    }

    // Check 2: Chain starts from factory
    if let Some(first_event) = history.custody_events.first() {
        if !matches!(first_event.event_type, CustodyEventType::Created) {
            issues.push("First event is not a creation event".to_string());
        }
    }

    // Check 3: No gaps in custody chain
    for i in 1..history.custody_events.len() {
        let prev = &history.custody_events[i - 1];
        let curr = &history.custody_events[i];

        if prev.to_location != curr.from_location.clone().unwrap_or_default() {
            issues.push(format!(
                "Custody gap between {} and {}",
                prev.to_location,
                curr.from_location.clone().unwrap_or_default()
            ));
        }
    }

    // Check 4: All inspections passed
    for inspection in &history.inspections {
        if matches!(inspection.result, InspectionResult::Failed) {
            issues.push(format!(
                "Failed inspection at {}",
                inspection.location_id
            ));
        }
    }

    let is_authentic = issues.is_empty();

    Ok(AuthenticityResult {
        is_authentic,
        confidence: if is_authentic { 1.0 } else { 0.5 },
        issues,
        total_custody_events: history.custody_events.len(),
        total_inspections: history.inspections.len(),
    })
}

// ============================================================================
// Response Types
// ============================================================================

#[derive(Serialize, Deserialize, Debug)]
pub struct ProductHistory {
    pub product: Product,
    pub custody_events: Vec<CustodyEvent>,
    pub inspections: Vec<InspectionRecord>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct AuthenticityResult {
    pub is_authentic: bool,
    pub confidence: f32,
    pub issues: Vec<String>,
    pub total_custody_events: usize,
    pub total_inspections: usize,
}

// ============================================================================
// Helper Functions
// ============================================================================

fn anchor_hash(anchor: &str) -> ExternResult<EntryHash> {
    hash_entry(anchor.to_string())
}

fn get_product_hash(product_id: &str) -> ExternResult<Option<ActionHash>> {
    let anchor = anchor_hash("all_products")?;
    let links = get_links(anchor, LinkTypes::AllProducts, Some(LinkTag::new(product_id.as_bytes())))?;

    Ok(links.first().and_then(|l| l.target.clone().into_action_hash()))
}

fn get_location_hash(location_id: &str) -> ExternResult<Option<ActionHash>> {
    let anchor = anchor_hash("all_locations")?;
    let links = get_links(anchor, LinkTypes::AllLocations, Some(LinkTag::new(location_id.as_bytes())))?;

    Ok(links.first().and_then(|l| l.target.clone().into_action_hash()))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_product_serialization() {
        let product = Product {
            product_id: "SKU-12345".to_string(),
            name: "Organic Coffee Beans".to_string(),
            category: "Food & Beverage".to_string(),
            manufacturer: Manufacturer {
                name: "Fair Trade Co".to_string(),
                location: "Colombia".to_string(),
                certifications: vec!["USDA Organic".to_string(), "Fair Trade".to_string()],
            },
            attributes: serde_json::json!({
                "weight_kg": 1.0,
                "origin": "Huila Region",
                "roast": "Medium"
            }),
            created_at: 1702500000000,
        };

        let json = serde_json::to_string(&product).unwrap();
        assert!(json.contains("SKU-12345"));
        assert!(json.contains("Fair Trade"));
    }

    #[test]
    fn test_environment_conditions() {
        let conditions = EnvironmentConditions {
            temperature: Some(4.0),
            humidity: Some(45.0),
            light: None,
            shock: Some(false),
            sensor_id: Some("cold-chain-001".to_string()),
        };

        let json = serde_json::to_string(&conditions).unwrap();
        assert!(json.contains("4.0"));
    }
}
