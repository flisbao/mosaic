pub mod application;
pub mod domain;
pub mod infrastructure;
pub mod presentation;

#[cfg(test)]
mod tests {
    #[test]
    fn crate_exposes_all_layers() {
        assert!(std::path::Path::new("src/domain/mod.rs").exists());
        assert!(std::path::Path::new("src/application/mod.rs").exists());
        assert!(std::path::Path::new("src/infrastructure/mod.rs").exists());
        assert!(std::path::Path::new("src/presentation/mod.rs").exists());
    }
}
