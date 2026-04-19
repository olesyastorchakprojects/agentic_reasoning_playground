#[derive(Debug, Clone)]
pub struct UserRequest {
    pub query: String,
}

#[derive(Debug, Clone)]
pub struct NormalizedUserRequest {
    pub query: String,
    pub input_token_count: usize,
}
