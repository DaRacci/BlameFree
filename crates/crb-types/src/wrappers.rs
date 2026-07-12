pub trait WrappedData {
    fn get(&self) -> &str;
}

pub struct Diff(pub String);

impl WrappedData for Diff {
    fn get(&self) -> &str {
        &self.0
    }
}

pub struct Prompt(pub String);

impl WrappedData for Prompt {
    fn get(&self) -> &str {
        &self.0
    }
}

pub struct Model(pub String);

impl WrappedData for Model {
    fn get(&self) -> &str {
        &self.0
    }
}
