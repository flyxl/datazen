#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProductFeatureKey {
    SchemaDiff,
    DataSync,
    DataTransfer,
}

impl ProductFeatureKey {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SchemaDiff => "schemaDiff",
            Self::DataSync => "dataSync",
            Self::DataTransfer => "dataTransfer",
        }
    }
}

pub const PRODUCT_FEATURE_KEYS: [ProductFeatureKey; 3] = [
    ProductFeatureKey::SchemaDiff,
    ProductFeatureKey::DataSync,
    ProductFeatureKey::DataTransfer,
];

pub fn is_product_feature_enabled(key: ProductFeatureKey) -> bool {
    match key {
        ProductFeatureKey::SchemaDiff
        | ProductFeatureKey::DataSync
        | ProductFeatureKey::DataTransfer => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_product_feature_keeps_its_wire_literal_and_is_enabled() {
        let literals = ["schemaDiff", "dataSync", "dataTransfer"];
        assert_eq!(PRODUCT_FEATURE_KEYS.len(), 3);
        for (key, literal) in PRODUCT_FEATURE_KEYS.iter().zip(literals) {
            assert_eq!(key.as_str(), literal);
            assert!(is_product_feature_enabled(*key));
        }
    }
}
