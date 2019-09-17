use serde_derive::Deserialize;
use serde_json;
use std::fmt;
use transient_hashmap::TransientHashMap;

pub struct KeybaseProfilePictureCache {
  cache: TransientHashMap<String, String>,
}

#[derive(Debug)]
pub enum KBProfileError {
  Simple(String),
  Reqwest(reqwest::Error),
  Parsing(serde_json::error::Error),
}

impl fmt::Display for KBProfileError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{:?}", self)
  }
}

impl From<reqwest::Error> for KBProfileError {
  fn from(e: reqwest::Error) -> Self {
    KBProfileError::Reqwest(e)
  }
}
impl From<serde_json::error::Error> for KBProfileError {
  fn from(e: serde_json::error::Error) -> Self {
    KBProfileError::Parsing(e)
  }
}

impl std::error::Error for KBProfileError {}

impl Default for KeybaseProfilePictureCache {
  fn default() -> Self {
    KeybaseProfilePictureCache {
      cache: TransientHashMap::new(60 * 60),
    }
  }
}

#[derive(Deserialize, Debug)]
struct KeybaseProfileResp {
  status: KeybaseRespStatus,
  them: Vec<KeybaseProfile>,
}

#[derive(Deserialize, Debug)]
struct KeybaseRespStatus {
  code: i32,
  name: String,
}

#[derive(Deserialize, Debug)]
struct KeybaseProfile {
  pictures: Option<KeybasePics>,
}

#[derive(Deserialize, Debug)]
struct KeybasePics {
  primary: Option<KeybasePicPrimary>,
}

#[derive(Deserialize, Debug)]
struct KeybasePicPrimary {
  url: Option<String>,
}

impl KeybaseProfilePictureCache {
  pub fn get_keybase_profile_picture(
    &mut self,
    username: &String,
  ) -> Result<&String, KBProfileError> {
    if self.cache.contains_key(username) {
      Ok(self.cache.get(username).unwrap())
    } else {
      let body = reqwest::get(&format!(
        "https://keybase.io/_/api/1.0/user/lookup.json?usernames={}",
        username
      ))?
      .text()?;
      let resp: KeybaseProfileResp = serde_json::from_str(&body)?;
      match resp.them.first() {
        Some(KeybaseProfile {
          pictures:
            Some(KeybasePics {
              primary:
                Some(KeybasePicPrimary {
                  url: Some(profile_pic),
                  ..
                }),
            }),
          ..
        }) => {
          self.cache.insert(username.clone(), profile_pic.clone());
          Ok(self.cache.get(username).unwrap())
        }
        _ => {
          println!("KB API did not give us a profile pic");
          Err(KBProfileError::Simple("Not in cache".into()))
        }
      }
    }
  }
}
