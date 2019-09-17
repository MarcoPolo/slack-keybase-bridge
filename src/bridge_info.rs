use serde_derive::Deserialize;
use std::env;
use std::fs::File;
use std::io::Read;

#[derive(Deserialize)]
pub struct BridgeInfo {
  pub slack: OAuth,
  pub slackbot: OAuth,
  pub keybase: KeybaseInfo,
}

#[derive(Deserialize)]
pub struct OAuth {
  pub oauth_access_token: String,
}

#[derive(Deserialize)]
pub struct KeybaseInfo {
  pub paper_key: String,
  pub team: String,
  pub bot_name: String,
}

pub fn get_bridge_info() -> BridgeInfo {
  let bridge_config_path = if let Ok(p) = env::var("BRIDGE_CONFIG") {
    p
  } else {
    println!("You didn't supply a bridge_config for me to use.");
    println!("Set the environment variable of BRIDGE_CONFIG to the path of the config file.");
    println!("Bonus points if you use kbfs to securely store the config! (It contains secrets, so don't commit it!)");
    println!(
      "e.g.: `BRIDGE_CONFIG=/keybase/private/marcopolo,cryptic_msngr/bridge_config.toml cargo run`"
    );
    panic!("Missing BRIDGE_CONFIG environment variable");
  };
  println!("Path is {}", bridge_config_path);
  let mut file = File::open(bridge_config_path).expect("Missing bridge_config.toml");
  let mut contents = String::new();
  file.read_to_string(&mut contents).unwrap();
  toml::from_str(&contents).expect("Couldn't parse bridge_config.toml")
}
