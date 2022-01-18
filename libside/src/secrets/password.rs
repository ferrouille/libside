use rand::prelude::*;
use serde::{Deserialize, Serialize};
use std::marker::PhantomData;

use super::Secret;

pub struct Alphanumeric;

impl PasswordKind for Alphanumeric {
    type Distribution = rand::distributions::Alphanumeric;

    fn distribution() -> Self::Distribution {
        rand::distributions::Alphanumeric
    }
}

pub struct Alphabetic;

impl PasswordKind for Alphabetic {
    type Distribution = Self;

    fn distribution() -> Self::Distribution {
        Alphabetic
    }
}

impl Distribution<u8> for Alphabetic {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> u8 {
        const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
        CHARS[rng.gen_range(0..CHARS.len())]
    }
}

pub trait PasswordKind {
    type Distribution: Distribution<u8>;

    fn distribution() -> Self::Distribution;
}

#[derive(Serialize, Deserialize)]
pub struct Password<const LENGTH: usize, K: PasswordKind> {
    pass: String,
    _phantom: PhantomData<K>,
}

impl<const LENGTH: usize, K: PasswordKind> AsRef<str> for Password<LENGTH, K> {
    fn as_ref(&self) -> &str {
        &self.pass
    }
}

impl<const LENGTH: usize, K: PasswordKind> std::fmt::Debug for Password<LENGTH, K> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_ref())
    }
}

impl<const LENGTH: usize, K: PasswordKind> Password<LENGTH, K> {
    pub fn get(&self) -> &str {
        &self.pass
    }
}

impl<const LENGTH: usize, K: PasswordKind> Clone for Password<LENGTH, K> {
    fn clone(&self) -> Self {
        Self {
            pass: self.pass.clone(),
            _phantom: PhantomData,
        }
    }
}

impl<const LENGTH: usize, K: PasswordKind> Secret for Password<LENGTH, K> {
    const KIND: &'static str = "password";

    fn generate_new() -> Self {
        let pass = rand::thread_rng()
            .sample_iter(&K::distribution())
            .take(LENGTH)
            .map(char::from)
            .collect();

        Password {
            pass,
            _phantom: PhantomData,
        }
    }
}
