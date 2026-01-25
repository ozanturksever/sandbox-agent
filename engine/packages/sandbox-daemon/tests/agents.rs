use std::collections::HashMap;

use sandbox_daemon_core::agents::{AgentId, AgentManager, InstallOptions, SpawnOptions};
use sandbox_daemon_core::credentials::{extract_all_credentials, CredentialExtractionOptions};

fn build_env() -> HashMap<String, String> {
    let options = CredentialExtractionOptions::new();
    let credentials = extract_all_credentials(&options);
    let mut env = HashMap::new();
    if let Some(anthropic) = credentials.anthropic {
        env.insert("ANTHROPIC_API_KEY".to_string(), anthropic.api_key);
    }
    if let Some(openai) = credentials.openai {
        env.insert("OPENAI_API_KEY".to_string(), openai.api_key);
    }
    env
}

fn amp_configured() -> bool {
    let home = dirs::home_dir().unwrap_or_default();
    home.join(".amp").join("config.json").exists()
}

#[test]
fn test_agents_install_version_spawn() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempfile::tempdir()?;
    let manager = AgentManager::new(temp_dir.path().join("bin"))?;
    let env = build_env();
    assert!(!env.is_empty(), "expected credentials to be available");

    let agents = [AgentId::Claude, AgentId::Codex, AgentId::Opencode, AgentId::Amp];
    for agent in agents {
        let install = manager.install(agent, InstallOptions::default())?;
        assert!(install.path.exists(), "expected install for {agent}");
        let version = manager.version(agent)?;
        assert!(version.is_some(), "expected version for {agent}");

        if agent != AgentId::Amp || amp_configured() {
            let mut spawn = SpawnOptions::new("Respond with exactly the text OK and nothing else.");
            spawn.env = env.clone();
            let result = manager.spawn(agent, spawn)?;
            assert!(
                result.status.success(),
                "spawn failed for {agent}: {}",
                result.stderr
            );
            let output = format!("{}{}", result.stdout, result.stderr);
            assert!(output.contains("OK"), "expected OK for {agent}, got: {output}");
        }
    }

    Ok(())
}
