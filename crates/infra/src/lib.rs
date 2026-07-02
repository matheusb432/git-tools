//! Concrete adapters (Ports & Adapters "outside"). Owns all storage/process/OS
//! integrations; the application core sees only ports. Currently: the
//! content-addressed diff store (formerly the standalone `gtl-store` crate).

pub mod store;
