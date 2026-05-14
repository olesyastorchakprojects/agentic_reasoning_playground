#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UserRequest {
    pub query: String,
    pub golden_question: Option<GoldenQuestion>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GoldenQuestion {
    pub case_id: String,
    pub query: GoldenQuestionQuery,
    pub expected_query_structuring: GoldenQueryStructuringTargets,
    pub expected_candidate_cards: GoldenCandidateCardSection,
    pub expected_incident_evidence: GoldenIncidentEvidenceTargets,
    pub expected_theory_evidence: GoldenTheoryEvidenceTargets,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GoldenObservationSpec {
    pub observation_id: String,
    pub raw: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GoldenQuestionQuery {
    pub raw: String,
    #[serde(default)]
    pub observations: Vec<GoldenObservationSpec>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GoldenQueryStructuringTargets {
    pub symptoms: GoldenVocabularyFieldTargets,
    pub affected_subsystems: GoldenVocabularyFieldTargets,
    pub failure_modes: GoldenVocabularyFieldTargets,
    pub system_properties: GoldenVocabularyFieldTargets,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GoldenVocabularyFieldTargets {
    pub strict_vocabulary_terms: Vec<String>,
    pub soft_vocabulary_terms: Vec<String>,
    pub graded_relevance: Vec<GoldenTermRelevance>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GoldenTermRelevance {
    pub term: String,
    pub score: f32,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GoldenCandidateCardSection {
    pub retrieval_relevant_cards: GoldenCardRetrievalTargets,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GoldenCardRetrievalTargets {
    pub strict_card_ids: Vec<String>,
    pub soft_card_ids: Vec<String>,
    pub graded_relevance: Vec<GoldenCardRelevance>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GoldenCardRelevance {
    pub card_id: String,
    pub score: f32,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GoldenIncidentEvidenceTargets {
    pub primary_card_evidence_query: GoldenChunkRetrievalCallTargets,
    pub alternative_cards_evidence_query: GoldenChunkRetrievalCallTargets,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GoldenChunkRetrievalCallTargets {
    pub retrieval_call_id: String,
    pub relevance_judgments: GoldenChunkRetrievalTargets,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GoldenTheoryEvidenceTargets {
    pub mechanism_explanation: GoldenChunkRetrievalTargets,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GoldenChunkRetrievalTargets {
    pub strict_chunk_ids: Vec<String>,
    pub soft_chunk_ids: Vec<String>,
    pub graded_relevance: Vec<GoldenChunkRelevance>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GoldenChunkRelevance {
    pub chunk_id: String,
    pub score: f32,
}

#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct RetrievalEvaluationMetrics {
    pub evaluated_k: u32,
    pub recall_soft: f32,
    pub recall_strict: f32,
    pub rr_soft: f32,
    pub rr_strict: f32,
    pub ndcg: f32,
    pub first_relevant_rank_soft: Option<u32>,
    pub first_relevant_rank_strict: Option<u32>,
    pub num_relevant_soft: u32,
    pub num_relevant_strict: u32,
}

#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct RetrievalCallStats {
    pub hits_count: u32,
    pub selected_count: u32,
    pub top_score: Option<f32>,
    pub min_score: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct CandidateCardRetrievalMetrics {
    pub retrieval_relevant_cards: RetrievalEvaluationMetrics,
    pub call_stats: RetrievalCallStats,
}

#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct IncidentEvidenceBranchRetrievalMetrics {
    pub relevance_judgments: RetrievalEvaluationMetrics,
    pub call_stats: RetrievalCallStats,
}

#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct IncidentEvidenceRetrievalMetrics {
    pub primary_card_evidence_query: IncidentEvidenceBranchRetrievalMetrics,
    pub alternative_cards_evidence_query: IncidentEvidenceBranchRetrievalMetrics,
}

#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct TheoryEvidenceRetrievalMetrics {
    pub mechanism_explanation: RetrievalEvaluationMetrics,
    pub call_stats: RetrievalCallStats,
}

#[derive(Debug, Clone)]
pub struct OpenInferenceContext {
    pub root_span: tracing::Span,
}

#[derive(Debug, Clone)]
pub struct Context {
    pub open_inference: OpenInferenceContext,
    pub golden_question: Option<GoldenQuestion>,
}

impl Context {
    pub fn new(
        open_inference: OpenInferenceContext,
        golden_question: Option<GoldenQuestion>,
    ) -> Self {
        Self {
            open_inference,
            golden_question,
        }
    }

    pub fn noop() -> Self {
        Self {
            open_inference: OpenInferenceContext {
                root_span: tracing::Span::none(),
            },
            golden_question: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct IncidentPhase {
    pub phase_name: String,
    pub context: String,
    pub symptoms: Vec<String>,
    pub user_visible_impact: Vec<String>,
    pub observations: Vec<String>,
    pub actions_taken: Vec<String>,
    pub changes_after_actions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DiscriminatingCheck {
    pub question: String,
    pub why: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ExpectedObservation {
    pub observation: String,
    pub effect: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct IncidentCard {
    pub case_id: String,
    pub title: String,
    pub source_type: String,
    pub source_name: String,
    pub source_path: String,
    pub vendor_or_project: Option<String>,
    pub system_type: Option<String>,
    pub version_tested: Option<String>,
    pub report_date: Option<String>,
    pub short_summary: String,
    pub canonical_symptoms: Vec<String>,
    pub affected_components: Vec<String>,
    pub failure_mode_candidates: Vec<String>,
    pub observed_phases: Vec<String>,
    pub incident_phases: Vec<IncidentPhase>,
    pub turning_points: Vec<String>,
    pub candidate_explanations: Vec<String>,
    pub diagnostic_patterns: Vec<String>,
    pub discriminating_checks: Vec<DiscriminatingCheck>,
    pub expected_observations: Vec<ExpectedObservation>,
    pub investigation_steps: Vec<String>,
    pub root_cause_summary: Option<String>,
    pub reasoning_summary: Option<String>,
    pub mitigations_or_workarounds: Vec<String>,
    pub prevention_or_design_followups: Vec<String>,
    pub claimed_guarantees: Vec<String>,
    pub violated_properties: Vec<String>,
    pub resolution_status: Option<String>,
    pub fix_versions: Vec<String>,
    pub confidence_notes: Vec<String>,
    pub source_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NormalizedUserRequest {
    pub query: String,
    pub input_token_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RetrievalQueryInput {
    pub query_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StructuredUserQuery {
    pub intent: String,
    pub scenario: String,
    pub symptoms: Vec<StructuredUserQueryTerm>,
    pub affected_subsystems: Vec<StructuredUserQueryTerm>,
    pub failure_modes: Vec<StructuredUserQueryTerm>,
    pub system_properties: Vec<StructuredUserQueryTerm>,
    pub entities: Vec<String>,
    pub constraints: Vec<String>,
    pub triggers: Vec<String>,
    pub observability_signals: Vec<String>,
    pub unresolved_terms: Vec<String>,
    pub rejected_nearby_terms: Vec<RejectedNearbyTerm>,
    pub confidence: StructuredUserQueryConfidence,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct QueryStructuringOutput {
    pub structured_query: StructuredUserQuery,
    pub token_usage: ModelTokenUsage,
    pub metrics: Option<QueryStructuringMetrics>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct QueryStructuringControlledVocabulary {
    pub canonical_symptoms: Vec<String>,
    pub affected_components: Vec<String>,
    pub failure_mode_candidates: Vec<String>,
    pub violated_properties: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct QueryStructuringMetrics {
    pub top_level: QueryStructuringTopLevelMetrics,
    pub vocab_fields: QueryStructuringVocabularyFieldMetrics,
    pub non_vocab_fields: QueryStructuringNonVocabularyFieldMetrics,
    pub aggregates: QueryStructuringAggregateMetrics,
}

#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct QueryStructuringTopLevelMetrics {
    pub macro_precision_soft: f32,
    pub macro_recall_strict: f32,
    pub macro_recall_soft: f32,
    pub overall_grounded_strict_recall: f32,
    pub all_fields_core_success_rate: f32,
}

#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct QueryStructuringVocabularyFieldMetrics {
    pub symptoms: QueryStructuringVocabularyFieldMetricSet,
    pub affected_subsystems: QueryStructuringVocabularyFieldMetricSet,
    pub failure_modes: QueryStructuringVocabularyFieldMetricSet,
    pub system_properties: QueryStructuringVocabularyFieldMetricSet,
}

#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct QueryStructuringVocabularyFieldMetricSet {
    pub invalid_vocab_count: u32,
    pub duplicate_term_count: u32,

    pub precision_soft: f32,
    pub recall_strict: f32,
    pub recall_soft: f32,
    pub num_false_positive: u32,
    pub num_false_negative_strict: u32,
    pub num_predicted_terms: u32,

    pub graded_coverage: f32,
    pub average_selected_score: f32,
    pub zero_score_selection_count: u32,

    pub grounded_strict_recall: f32,
    pub unsupported_selected_term_rate: f32,
    pub missing_evidence_span_count: u32,
    pub invalid_evidence_span_count: u32,
    pub evidence_span_near_substring_rate: f32,

    pub weak_inference_rate: f32,
    pub strict_terms_weak_inference_rate: f32,
    pub weak_false_positive_rate: f32,

    pub field_core_success: bool,
    pub field_grounded_success: bool,
    pub empty_when_gold_exists: bool,
}

#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct QueryStructuringNonVocabularyFieldMetrics {
    pub entities_count: u32,
    pub constraints_count: u32,
    pub triggers_count: u32,
    pub observability_signals_count: u32,
    pub unresolved_terms_count: u32,
    pub intent_present: bool,
    pub scenario_present: bool,
}

#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct QueryStructuringAggregateMetrics {
    pub macro_precision_soft: f32,
    pub macro_recall_strict: f32,
    pub macro_recall_soft: f32,
    pub overall_grounded_strict_recall: f32,
    pub all_fields_core_success_rate: f32,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ModelTokenUsage {
    pub prompt_tokens: Option<usize>,
    pub completion_tokens: Option<usize>,
    pub total_tokens: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StructuredUserQueryTerm {
    pub term: String,
    pub evidence_span: String,
    pub support_level: StructuredUserQuerySupportLevel,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RejectedNearbyTerm {
    pub term: String,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StructuredUserQuerySupportLevel {
    Explicit,
    StrongParaphrase,
    WeakInference,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StructuredUserQueryConfidence {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CandidateCard {
    pub case_id: String,
    pub score: f32,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CandidateCardRetrievalOutput {
    pub ranked_candidates: Vec<CandidateCard>,
    pub primary: Option<CandidateCard>,
    pub alternatives: Vec<CandidateCard>,
    pub metrics: Option<CandidateCardRetrievalMetrics>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PrimaryCardStatus {
    Tentative,
    Sticky,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CardBranchRerankingOutput {
    pub primary_card_id: String,
    pub primary_card_status: PrimaryCardStatus,
    pub alternative_card_ids: Vec<String>,
    pub challenger_card_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum IterationProfile {
    Initial,
    Continuation,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct IncidentEvidenceCardBranchesInput {
    pub primary_card_id: String,
    pub alternative_card_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CardHydrationOutput {
    pub primary: Option<IncidentCard>,
    pub alternatives: Vec<IncidentCard>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct HydratedCardBranchesInput {
    pub primary: IncidentCard,
    pub alternatives: Vec<IncidentCard>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct IncidentEvidenceChunk {
    pub chunk_id: String,
    pub case_id: String,
    pub score: f32,
    pub chunk_tags: Vec<String>,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct IncidentEvidenceRetrievalOutput {
    pub primary_chunks: Vec<IncidentEvidenceChunk>,
    pub alternative_chunks: Vec<IncidentEvidenceChunk>,
    pub metrics: Option<IncidentEvidenceRetrievalMetrics>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TheoryEvidenceChunk {
    pub chunk_id: String,
    pub score: f32,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TheoryEvidenceRetrievalOutput {
    pub chunks: Vec<TheoryEvidenceChunk>,
    pub metrics: Option<TheoryEvidenceRetrievalMetrics>,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    strum_macros::EnumString,
    strum_macros::Display,
    strum_macros::AsRefStr,
    serde::Serialize,
    serde::Deserialize,
)]
pub enum IncidentChunkTag {
    #[strum(serialize = "chunk_role:symptom")]
    Symptom,
    #[strum(serialize = "chunk_role:impact")]
    Impact,
    #[strum(serialize = "chunk_role:timeline")]
    Timeline,
    #[strum(serialize = "chunk_role:symptom_change")]
    SymptomChange,
    #[strum(serialize = "chunk_role:investigation")]
    Investigation,
    #[strum(serialize = "chunk_role:diagnostic_step")]
    DiagnosticStep,
    #[strum(serialize = "chunk_role:hypothesis_update")]
    HypothesisUpdate,
    #[strum(serialize = "chunk_role:recovery")]
    Recovery,
    #[strum(serialize = "chunk_role:failure_mode")]
    FailureMode,
    #[strum(serialize = "chunk_role:root_cause")]
    RootCause,
    #[strum(serialize = "chunk_role:contributing_factor")]
    ContributingFactor,
    #[strum(serialize = "chunk_role:uncertainty")]
    Uncertainty,
    #[strum(serialize = "chunk_role:lesson")]
    Lesson,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum PromptEvidenceRole {
    EvidenceForMatch,
    FirstCheckHint,
    SupportingExplanation,
    AlternativeContext,
    MechanismExplanation,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PromptIncidentEvidenceChunk {
    pub role: PromptEvidenceRole,
    pub chunk_id: String,
    pub case_id: String,
    pub score: f32,
    pub chunk_tags: Vec<IncidentChunkTag>,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PromptTheoryEvidenceChunk {
    pub role: PromptEvidenceRole,
    pub chunk_id: String,
    pub score: f32,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct EvidenceTopology {
    pub primary_evidence_roles: Vec<String>,
    pub alternative_context_present: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alternative_context_case_ids: Vec<String>,
    pub theory_evidence_present: bool,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PromptContextAssemblyOutput {
    pub prompt: String,
    pub response_schema: serde_json::Value,
    pub evidence_topology: EvidenceTopology,
    pub incident_evidence_chunks: Vec<PromptIncidentEvidenceChunk>,
    pub theory_chunks: Vec<PromptTheoryEvidenceChunk>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LlmStructuredGenerationOutput {
    pub response_json: serde_json::Value,
    pub token_usage: ModelTokenUsage,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ResponseValidationAndNormalizationOutput {
    pub response: DiagnosticResponse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum AdequacyStatus {
    Blocking,
    WeakButRunnable,
    Sufficient,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum MissingInformationTopic {
    SymptomDescription,
    AffectedComponent,
    TriggerOrRecentChange,
    FailureMechanismHint,
    ExpectedVsActual,
    ObservedResult,
    ExecutionContext,
    CheckOutcome,
    ScopeOrBlastRadius,
    CorrectionTarget,
    TermClarification,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AdequacyAssessment {
    pub status: AdequacyStatus,
    pub missing_information_topics: Vec<MissingInformationTopic>,
    pub follow_up_questions: Vec<String>,
    pub summary_reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct HypothesisId(pub uuid::Uuid);

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum HypothesisEvidenceSource {
    PrimaryIncident,
    AlternativeContext,
    TheoryMechanism,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum HypothesisStatus {
    Active,
    Weakened,
    Rejected(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Confidence {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Hypothesis {
    pub id: HypothesisId,
    pub text: String,
    pub status: HypothesisStatus,
    pub source: HypothesisEvidenceSource,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DiagnosticResponse {
    pub problem_understanding: String,
    pub similar_practical_context: String,
    pub hypotheses: Vec<Hypothesis>,
    pub first_check: String,
    pub result_interpretation: DiagnosticResultInterpretation,
    pub competing_interpretation: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ResolvedObservation {
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ObservationBoundaryResolution {
    Supported(ResolvedObservation),
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ObservationBoundaryResolverOutput {
    pub normalized_user_input: String,
    pub confidence: Confidence,
    pub reason: String,
    pub resolution: ObservationBoundaryResolution,
    pub token_usage: ModelTokenUsage,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DiagnosticResultInterpretation {
    pub supports_primary_if: String,
    pub supports_competing_if: String,
    pub inconclusive_if: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RuntimeLlmStageConfigSnapshot {
    pub provider: String,
    pub model_name: String,
    pub input_cost_per_million_tokens: f64,
    pub output_cost_per_million_tokens: f64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RunConfigSnapshot {
    pub query_structuring: RuntimeLlmStageConfigSnapshot,
    pub observation_boundary_resolver: RuntimeLlmStageConfigSnapshot,
    pub observation_extraction: RuntimeLlmStageConfigSnapshot,
    pub llm_structured_generation: RuntimeLlmStageConfigSnapshot,
    pub query_structuring_prompt_version: String,
    pub observation_boundary_resolver_prompt_version: String,
    pub observation_extraction_prompt_version: String,
    pub prompt_context_prompt_version: String,
    pub diagnostic_update_prompt_context_prompt_version: String,
    pub retrieval_cards_top_k: usize,
    pub retrieval_cards_collection: String,
    pub retrieval_practice_top_k: usize,
    pub retrieval_practice_collection: String,
    pub retrieval_theory_top_k: usize,
    pub retrieval_theory_collection: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationPolarity {
    Present,
    Absent,
    Corrected,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ExtractedObservation {
    pub statement: String,
    pub confidence: Confidence,
    pub condition: Option<String>,
    pub polarity: ObservationPolarity,
    pub time_relation: Option<String>,
    pub source_span: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ObservationExtractionOutput {
    pub normalized_user_input: String,
    pub resolved_observation: ResolvedObservation,
    pub confidence: Confidence,
    pub observations: Vec<ExtractedObservation>,
    pub needs_more_context: bool,
    pub missing_context_questions: Vec<String>,
    pub token_usage: ModelTokenUsage,
}

mod card_selection_context;
pub use card_selection_context::{
    CardSelectionContext, CardSelectionContextError, CardSelectionSnapshot,
};

mod diagnostic_context;
pub use diagnostic_context::{
    DiagnosticContext, DiagnosticContextError, HypothesisState, Observation, ObservationStatus,
    ProblemUnderstanding, ProblemUnderstandingSource, SuggestedCheck, TrackedHypothesis,
};
