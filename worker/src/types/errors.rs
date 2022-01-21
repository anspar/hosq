use std::fmt;

#[derive(Debug)]
pub enum CustomError {
    Inequality,
    InvalidAbiString
}

impl std::error::Error for CustomError {}

impl fmt::Display for CustomError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            CustomError::Inequality => write!(f, "Mismatching values"),
            CustomError::InvalidAbiString => write!(f, "Invalid abi encoded string"),
        }
    }
}
