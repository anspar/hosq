use std::fmt;

#[derive(Debug)]
pub enum CustomError {
    Inequality(String),
    InvalidAbiString
}

impl std::error::Error for CustomError {}

impl fmt::Display for CustomError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            CustomError::Inequality(m) => write!(f, "Mismatching values: {}", m),
            CustomError::InvalidAbiString => write!(f, "Invalid abi encoded string"),
        }
    }
}
