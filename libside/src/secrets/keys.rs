use std::{
    convert::TryInto,
    io::{self, Write},
};

use super::Secret;
use openssl::{bn::BigNumRef, rsa::Rsa};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AsymmetricKey<const BITS: u32> {
    public: String,
    private: String,
}

impl<const BITS: u32> AsymmetricKey<BITS> {
    pub fn private_key_data(&self) -> &str {
        &self.private
    }

    pub fn public_key_data(&self, name: &str) -> String {
        format!("ssh-rsa {} {}", self.public, name)
    }
}

impl<const BITS: u32> Secret for AsymmetricKey<BITS> {
    const KIND: &'static str = "asymmetric-key";

    fn generate_new() -> Self {
        let key = Rsa::generate(BITS).unwrap();
        let private = std::str::from_utf8(&key.private_key_to_pem().unwrap())
            .unwrap()
            .to_owned();

        let mut data = Vec::new();
        write_buf(&mut data, "ssh-rsa".as_bytes()).unwrap();
        write_bignum(&mut data, key.e()).unwrap();
        write_bignum(&mut data, key.n()).unwrap();

        let public = base64::encode(&data);

        AsymmetricKey { public, private }
    }
}

fn write_u32<W: Write>(w: &mut W, v: u32) -> io::Result<()> {
    w.write_all(&v.to_be_bytes())?;

    Ok(())
}

fn write_buf<W: Write>(w: &mut W, v: &[u8]) -> io::Result<()> {
    write_u32(w, v.len().try_into().expect("data too long"))?;
    w.write_all(v)?;

    Ok(())
}

fn write_bignum<W: Write>(w: &mut W, v: &BigNumRef) -> io::Result<()> {
    let mut data = v.to_vec();
    // Make sure the first bit is always 0
    if data[0] & 0x80 != 0 {
        data.insert(0, 0x00);
    }

    write_buf(w, &data)?;

    Ok(())
}
