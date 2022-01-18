use serde::{de::DeserializeOwned, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs,
    os::unix::prelude::PermissionsExt,
    path::Path,
};

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
    pub fn load(path: &Path) -> Result<Secrets, std::io::Error> {
        let mut result = Secrets {
            secrets: HashMap::new(),
            new_secrets: HashSet::new(),
        };

        for package_dir in fs::read_dir(path)? {
            let package_dir = package_dir?;

            for kind_dir in package_dir.path().read_dir()? {
                let kind_dir = kind_dir?;

                for entry_dir in kind_dir.path().read_dir()? {
                    let entry_path = entry_dir?;

                    let package = package_dir
                        .file_name()
                        .as_os_str()
                        .to_str()
                        .unwrap()
                        .to_string();
                    let kind = kind_dir
                        .file_name()
                        .as_os_str()
                        .to_str()
                        .unwrap()
                        .to_string();
                    let name = entry_path
                        .file_name()
                        .as_os_str()
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
                        .insert(internal_id, SecretData(fs::read(&entry_path.path())?));
                }
            }
        }

        Ok(result)
    }

    pub fn save(&mut self, path: &Path) -> Result<(), std::io::Error> {
        for item in self.new_secrets.iter() {
            let dir = path.join(&item.id.package).join(&item.kind);

            fs::create_dir_all(&dir)?;

            // Make sure nobody else can check what secrets exist
            let metadata = dir.metadata()?;
            let mut permissions = metadata.permissions();
            permissions.set_mode(0o700);
            fs::set_permissions(&dir, permissions)?;

            let file = dir.join(&item.id.name);
            fs::write(&file, &self.secrets.get(item).unwrap().0)?;

            // Also make sure the secret itself can't be read by anyone else
            let metadata = file.metadata()?;
            let mut permissions = metadata.permissions();
            permissions.set_mode(0o600);
            fs::set_permissions(file, permissions)?;
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
