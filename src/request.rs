#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct Request {
    pub key: String,
    pub val: crate::Value,
}

impl std::ops::Add for Request {
    type Output = Self;

    fn add(self, other: Self) -> Self::Output {
        Self {
            key: format!("{}/{}", self.key, other.key),
            val: other.val,
        }
    }
}
