use std::{error::Error, fmt};

#[derive(Debug)]
pub struct ManyErrors {
    errors: Vec<anyhow::Error>,
}

impl fmt::Display for ManyErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, error) in self.errors.iter().enumerate() {
            write!(f, "\n{}. {}", i + 1, error)?;
        }
        Ok(())
    }
}

impl Error for ManyErrors {}

impl ManyErrors {
    pub fn new() -> Self {
        Self { errors: Vec::new() }
    }

    pub fn push(&mut self, error: anyhow::Error) {
        self.errors.push(error);
    }

    pub fn is_empty(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn len(&self) -> usize {
        self.errors.len()
    }
}
