use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct GlobalConfig {
    pub debounce: Option<u64>,
    pub ignore: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct TaskConfig {
    pub name: String,
    pub watch: Vec<String>,
    pub run: String,
    pub restart: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct Config {
    pub global: Option<GlobalConfig>,
    pub task: Option<Vec<TaskConfig>>,
}

impl Config {
    pub fn from_toml(path: &str) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal() {
        let toml = r#"[global]
debounce = 10

[[task]]
name = "t"
watch = ["**/*.rs"]
run = "echo ok"
"#;
        let cfg: Config = toml::from_str(toml).unwrap();
        assert!(cfg.task.is_some());
    }
}
