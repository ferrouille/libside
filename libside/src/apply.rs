use crate::graph::{Applied, Graph, VerificationState};
use crate::requirements::Requirement;
use crate::system::System;
use std::fmt::Debug;
use std::fmt::Display;

#[derive(Debug, Clone)]
pub struct SystemState<R> {
    pub graph: Graph<R, Applied>,
}

impl<R: Requirement> Default for SystemState<R> {
    fn default() -> Self {
        Self {
            graph: Graph::new(),
        }
    }
}

impl<R: Requirement + Display> SystemState<R> {
    pub fn verify_system_state<'r, S: System>(
        &'r self,
        system: &mut S,
    ) -> Result<VerificationState<'r, R>, ()> {
        let seq = self.graph.generate_verify_sequence()?;
        seq.run(system)
    }
}
