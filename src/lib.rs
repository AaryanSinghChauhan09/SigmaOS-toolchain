pub struct ToolchainConfig;
impl ToolchainConfig {
    pub fn default_target_triple() -> &'static str { "x86_64-unknown-none" }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_toolchain_target() {
        assert_eq!(ToolchainConfig::default_target_triple(), "x86_64-unknown-none");
    }
}
