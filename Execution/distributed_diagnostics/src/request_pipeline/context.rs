#[derive(Debug, Clone)]
pub struct OpenInferenceContext {
    pub root_span: tracing::Span,
}

#[derive(Debug, Clone)]
pub struct Context {
    pub open_inference: OpenInferenceContext,
}

impl Context {
    pub fn new(open_inference: OpenInferenceContext) -> Self {
        Self { open_inference }
    }

    pub fn noop() -> Self {
        Self {
            open_inference: OpenInferenceContext {
                root_span: tracing::Span::none(),
            },
        }
    }
}

