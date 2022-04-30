use serde::{de::DeserializeOwned, Serialize};
use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

use crate::system::System;

pub mod keys;
pub mod password;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SecretId {
    package: String,
    name: String,
}

impl SecretId {
    pub fn new(package: String, name: String) -> SecretId {
        SecretId { package, name }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct InternalSecretId {
    id: SecretId,
    kind: String,
}

impl std::fmt::Display for InternalSecretId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}:{}", self.id.package, self.id.name, self.kind)
    }
}

pub struct SecretData(Vec<u8>);

pub struct Secrets {
    secrets: HashMap<InternalSecretId, SecretData>,
    new_secrets: HashSet<InternalSecretId>,
}

pub trait Secret: Serialize + DeserializeOwned + Clone {
    const KIND: &'static str;

    fn generate_new() -> Self;
}

impl Secrets {
    pub fn load<S: System>(path: &Path, system: &mut S) -> Result<Secrets, S::Error> {
        let mut result = Secrets {
            secrets: HashMap::new(),
            new_secrets: HashSet::new(),
        };

        for package_dir in system.read_dir(path)? {
            let package_dir = path.join(package_dir);

            for kind_dir in system.read_dir(&package_dir)? {
                let kind_dir = package_dir.join(&kind_dir);

                for entry_dir in system.read_dir(&kind_dir)? {
                    let entry_path = kind_dir.join(&entry_dir);

                    let package = package_dir
                        .file_name()
                        .unwrap()
                        .to_str()
                        .unwrap()
                        .to_string();
                    let kind = kind_dir.file_name().unwrap().to_str().unwrap().to_string();
                    let name = entry_path
                        .file_name()
                        .unwrap()
                        .to_str()
                        .unwrap()
                        .to_string();

                    let internal_id = InternalSecretId {
                        id: SecretId::new(package, name),
                        kind,
                    };

                    println!("  secret loaded: {}", internal_id);

                    result
                        .secrets
                        .insert(internal_id, SecretData(system.file_contents(&entry_path)?));
                }
            }
        }

        Ok(result)
    }

    pub fn save<S: System>(&mut self, path: &Path, system: &mut S) -> Result<(), S::Error> {
        for item in self.new_secrets.iter() {
            let dir = path.join(&item.id.package).join(&item.kind);

            system.make_dir_all(&dir)?;
            system.chmod(&dir, 0o700)?;

            let file = dir.join(&item.id.name);
            system.put_file_contents(&file, &self.secrets.get(item).unwrap().0)?;
            system.chmod(&file, 0o600)?;
        }

        self.new_secrets.clear();

        Ok(())
    }

    pub fn get_or_create<S: Secret + std::fmt::Debug>(
        &mut self,
        id: SecretId,
    ) -> Result<S, serde_json::Error> {
        let internal_id = InternalSecretId {
            id,
            kind: S::KIND.to_owned(),
        };

        if let Some(s) = self.secrets.get(&internal_id) {
            Ok(serde_json::from_slice(&s.0)?)
        } else {
            let new_secret = S::generate_new();
            self.secrets.insert(
                internal_id.clone(),
                SecretData(serde_json::to_vec(&new_secret.clone())?),
            );
            self.new_secrets.insert(internal_id.clone());

            println!("  secret generated: {} = {:?}", internal_id, new_secret);

            Ok(new_secret)
        }
    }
}
