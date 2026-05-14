use crate::shared_types::{
    AdequacyAssessment, AdequacyStatus, Confidence, MissingInformationTopic,
    ObservationBoundaryResolution, ObservationBoundaryResolverOutput, ObservationExtractionOutput,
    ObservationPolarity, StructuredUserQuery, StructuredUserQuerySupportLevel,
};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, thiserror::Error)]
pub enum InformationAdequacyAnalyzerError {
    #[error("structured user query is invalid: {0}")]
    InvalidStructuredUserQuery(String),

    #[error("observation extraction output is invalid: {0}")]
    InvalidObservationExtractionOutput(String),
}

// ─── Canonical question literals ──────────────────────────────────────────────

fn canonical_question(topic: MissingInformationTopic) -> &'static str {
    match topic {
        MissingInformationTopic::SymptomDescription =>
            "What exactly are you observing: errors, timeouts, retries, stale data, leader changes, or another visible failure?",
        MissingInformationTopic::AffectedComponent =>
            "Which component or subsystem seems involved: for example the database, broker, lock service, scheduler, API gateway, or another part of the system?",
        MissingInformationTopic::TriggerOrRecentChange =>
            "What changed right before this started: for example a deploy, restart, failover, scaling event, config change, or traffic spike?",
        MissingInformationTopic::FailureMechanismHint =>
            "What failure pattern does this look closest to: for example timeout handling, replication lag, leader-election instability, lock contention, or resource exhaustion?",
        MissingInformationTopic::ExpectedVsActual =>
            "What did you expect to happen, and what happened instead?",
        MissingInformationTopic::ObservedResult =>
            "What was the exact observed result: for example the error message, timeout behavior, empty result, retry loop, or recovery signal?",
        MissingInformationTopic::ExecutionContext =>
            "Where and when did you observe this: for example which node, component, request path, environment, or time window?",
        MissingInformationTopic::CheckOutcome =>
            "What was the result of the check you ran: for example what did the command, log, metric, or status output show?",
        MissingInformationTopic::ScopeOrBlastRadius =>
            "How wide is the impact: is this limited to one node, shard, or request path, or does it affect the whole system?",
        MissingInformationTopic::CorrectionTarget =>
            "Which earlier assumption are you correcting, and what is the corrected fact now?",
        MissingInformationTopic::TermClarification =>
            "Some terms are still ambiguous. Can you restate the issue using the exact observed behavior and concrete component names?",
    }
}

fn topics_to_questions(topics: &[MissingInformationTopic]) -> Vec<String> {
    topics.iter().map(|t| canonical_question(*t).to_string()).collect()
}

fn build_assessment(
    status: AdequacyStatus,
    topics: Vec<MissingInformationTopic>,
    summary_reason: &'static str,
) -> AdequacyAssessment {
    let follow_up_questions = topics_to_questions(&topics);
    AdequacyAssessment {
        status,
        missing_information_topics: topics,
        follow_up_questions,
        summary_reason: summary_reason.to_string(),
    }
}

// ─── Analyzer ─────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct InformationAdequacyAnalyzer;

impl InformationAdequacyAnalyzer {
    pub fn new() -> Self {
        Self
    }

    pub fn analyze_initial(
        &self,
        query: &StructuredUserQuery,
    ) -> Result<AdequacyAssessment, InformationAdequacyAnalyzerError> {
        let symptom_count = query.symptoms.len();
        let observability_count = query.observability_signals.len();
        let failure_mode_count = query.failure_modes.len();
        let trigger_count = query.triggers.len();
        let scope_count = query.affected_subsystems.len() + query.entities.len();
        let unresolved_count = query.unresolved_terms.len();
        let symptom_signal_count = symptom_count + observability_count;

        let diagnostic_anchor_count = {
            let has_symptom = symptom_signal_count > 0;
            let has_failure_mode = failure_mode_count > 0;
            let has_trigger = trigger_count > 0;
            let has_scope = scope_count > 0;
            [has_symptom, has_failure_mode, has_trigger, has_scope]
                .iter()
                .filter(|&&v| v)
                .count()
        };

        let weak_inference_term_count = [
            &query.symptoms,
            &query.affected_subsystems,
            &query.failure_modes,
            &query.system_properties,
        ]
        .iter()
        .flat_map(|v| v.iter())
        .filter(|t| t.support_level == StructuredUserQuerySupportLevel::WeakInference)
        .count();

        let explicit_term_count = [
            &query.symptoms,
            &query.affected_subsystems,
            &query.failure_modes,
            &query.system_properties,
        ]
        .iter()
        .flat_map(|v| v.iter())
        .filter(|t| t.support_level == StructuredUserQuerySupportLevel::Explicit)
        .count();

        // ── Blocking checks ────────────────────────────────────────────────────

        if symptom_signal_count == 0 {
            let topics = select_initial_topics(
                symptom_signal_count, scope_count, trigger_count, failure_mode_count,
                diagnostic_anchor_count, unresolved_count,
            );
            return Ok(build_assessment(
                AdequacyStatus::Blocking,
                topics,
                "The request does not describe any concrete symptom or observable behavior.",
            ));
        }

        if diagnostic_anchor_count <= 1 {
            let topics = select_initial_topics(
                symptom_signal_count, scope_count, trigger_count, failure_mode_count,
                diagnostic_anchor_count, unresolved_count,
            );
            return Ok(build_assessment(
                AdequacyStatus::Blocking,
                topics,
                "The request contains too little anchored diagnostic context to proceed safely.",
            ));
        }

        if symptom_signal_count > 0 && scope_count == 0 && trigger_count == 0 && failure_mode_count == 0 {
            let topics = select_initial_topics(
                symptom_signal_count, scope_count, trigger_count, failure_mode_count,
                diagnostic_anchor_count, unresolved_count,
            );
            return Ok(build_assessment(
                AdequacyStatus::Blocking,
                topics,
                "The request names a symptom but does not anchor it to component, trigger, or failure pattern context.",
            ));
        }

        if unresolved_count >= 2 && (symptom_signal_count == 0 || diagnostic_anchor_count == 1) {
            let topics = select_initial_topics(
                symptom_signal_count, scope_count, trigger_count, failure_mode_count,
                diagnostic_anchor_count, unresolved_count,
            );
            return Ok(build_assessment(
                AdequacyStatus::Blocking,
                topics,
                "The request remains too ambiguous because key terms are unresolved.",
            ));
        }

        // ── Weak checks ────────────────────────────────────────────────────────

        let is_weak = symptom_signal_count == 1
            || scope_count == 0
            || (trigger_count == 0 && failure_mode_count == 0)
            || weak_inference_term_count > explicit_term_count;

        if is_weak {
            let topics = select_initial_topics(
                symptom_signal_count, scope_count, trigger_count, failure_mode_count,
                diagnostic_anchor_count, unresolved_count,
            );
            return Ok(build_assessment(
                AdequacyStatus::WeakButRunnable,
                topics,
                "The request contains a usable signal but is still diagnostically thin.",
            ));
        }

        // ── Sufficient ─────────────────────────────────────────────────────────

        Ok(build_assessment(
            AdequacyStatus::Sufficient,
            vec![],
            "The request contains enough diagnostic context to continue.",
        ))
    }

    pub fn analyze_supported_observation(
        &self,
        observation: &ObservationExtractionOutput,
    ) -> Result<AdequacyAssessment, InformationAdequacyAnalyzerError> {
        let observation_count = observation.observations.len();
        let present_count = observation.observations.iter()
            .filter(|o| o.polarity == ObservationPolarity::Present).count();
        let absent_count = observation.observations.iter()
            .filter(|o| o.polarity == ObservationPolarity::Absent).count();
        let corrected_count = observation.observations.iter()
            .filter(|o| o.polarity == ObservationPolarity::Corrected).count();
        let question_count = observation.missing_context_questions.len();
        let medium_or_higher_count = observation.observations.iter()
            .filter(|o| matches!(o.confidence, Confidence::Medium | Confidence::High))
            .count();

        // ── Blocking checks ────────────────────────────────────────────────────

        if observation_count == 0 {
            let topics = select_observation_topics(
                observation_count, observation.needs_more_context, question_count,
                present_count, absent_count, corrected_count, &observation.confidence,
            );
            return Ok(build_assessment(
                AdequacyStatus::Blocking,
                topics,
                "The observation does not contain a concrete new diagnostic fact.",
            ));
        }

        if observation.needs_more_context {
            let topics = select_observation_topics(
                observation_count, observation.needs_more_context, question_count,
                present_count, absent_count, corrected_count, &observation.confidence,
            );
            return Ok(build_assessment(
                AdequacyStatus::Blocking,
                topics,
                "The observation requires more context before diagnostic update can proceed safely.",
            ));
        }

        if observation_count == 1 && observation.confidence == Confidence::Low {
            let topics = select_observation_topics(
                observation_count, observation.needs_more_context, question_count,
                present_count, absent_count, corrected_count, &observation.confidence,
            );
            return Ok(build_assessment(
                AdequacyStatus::Blocking,
                topics,
                "The observation is too weak and low-confidence for a safe diagnostic update.",
            ));
        }

        if corrected_count > 0
            && present_count == 0
            && absent_count == 0
            && observation_count == corrected_count
            && observation.confidence == Confidence::Low
        {
            let topics = select_observation_topics(
                observation_count, observation.needs_more_context, question_count,
                present_count, absent_count, corrected_count, &observation.confidence,
            );
            return Ok(build_assessment(
                AdequacyStatus::Blocking,
                topics,
                "The observation only corrects a prior assumption and is still too weak for a safe diagnostic update.",
            ));
        }

        // ── Weak checks ────────────────────────────────────────────────────────

        let is_weak = observation_count == 1
            || question_count > 0
            || medium_or_higher_count == 0
            || (corrected_count == observation_count && observation.confidence != Confidence::High);

        if is_weak {
            let topics = select_observation_topics(
                observation_count, observation.needs_more_context, question_count,
                present_count, absent_count, corrected_count, &observation.confidence,
            );
            return Ok(build_assessment(
                AdequacyStatus::WeakButRunnable,
                topics,
                "The observation contains a usable update signal but still lacks diagnostic strength.",
            ));
        }

        // ── Sufficient ─────────────────────────────────────────────────────────

        Ok(build_assessment(
            AdequacyStatus::Sufficient,
            vec![],
            "The observation contains enough concrete diagnostic information to continue.",
        ))
    }

    pub fn analyze_unsupported_observation(
        &self,
        boundary_output: &ObservationBoundaryResolverOutput,
    ) -> Result<AdequacyAssessment, InformationAdequacyAnalyzerError> {
        if matches!(boundary_output.resolution, ObservationBoundaryResolution::Supported(_)) {
            return Err(InformationAdequacyAnalyzerError::InvalidObservationExtractionOutput(
                "boundary_output.resolution is Supported, expected Unsupported".to_string(),
            ));
        }

        // Priority: ObservedResult, ExecutionContext, CheckOutcome — truncate to first 2
        let all_topics = [
            MissingInformationTopic::ObservedResult,
            MissingInformationTopic::ExecutionContext,
            MissingInformationTopic::CheckOutcome,
        ];
        let topics: Vec<MissingInformationTopic> = all_topics.iter().take(2).copied().collect();

        Ok(build_assessment(
            AdequacyStatus::Blocking,
            topics,
            "The latest user message is not yet a supported standalone diagnostic observation.",
        ))
    }
}

// ─── Topic selection helpers ──────────────────────────────────────────────────

fn select_initial_topics(
    symptom_signal_count: usize,
    scope_count: usize,
    trigger_count: usize,
    failure_mode_count: usize,
    diagnostic_anchor_count: usize,
    unresolved_count: usize,
) -> Vec<MissingInformationTopic> {
    let mut topics = Vec::new();

    // Priority 1: SymptomDescription
    if symptom_signal_count == 0 || symptom_signal_count == 1 {
        topics.push(MissingInformationTopic::SymptomDescription);
        if topics.len() >= 3 { return topics; }
    }

    // Priority 2: AffectedComponent
    if scope_count == 0 {
        topics.push(MissingInformationTopic::AffectedComponent);
        if topics.len() >= 3 { return topics; }
    }

    // Priority 3: TriggerOrRecentChange
    if trigger_count == 0 {
        topics.push(MissingInformationTopic::TriggerOrRecentChange);
        if topics.len() >= 3 { return topics; }
    }

    // Priority 4: FailureMechanismHint
    if failure_mode_count == 0 && diagnostic_anchor_count < 2 {
        topics.push(MissingInformationTopic::FailureMechanismHint);
        if topics.len() >= 3 { return topics; }
    }

    // Priority 5: ExpectedVsActual
    if symptom_signal_count > 0 && diagnostic_anchor_count < 2 {
        topics.push(MissingInformationTopic::ExpectedVsActual);
        if topics.len() >= 3 { return topics; }
    }

    // Priority 6: TermClarification
    if unresolved_count >= 2 {
        topics.push(MissingInformationTopic::TermClarification);
    }

    topics
}

fn select_observation_topics(
    observation_count: usize,
    needs_more_context: bool,
    question_count: usize,
    present_count: usize,
    absent_count: usize,
    corrected_count: usize,
    confidence: &Confidence,
) -> Vec<MissingInformationTopic> {
    let mut topics = Vec::new();

    // Priority 1: ObservedResult
    if observation_count == 0
        || (observation_count == 1 && *confidence == Confidence::Low)
    {
        topics.push(MissingInformationTopic::ObservedResult);
        if topics.len() >= 3 { return topics; }
    }

    // Priority 2: CheckOutcome
    if needs_more_context && question_count > 0 {
        topics.push(MissingInformationTopic::CheckOutcome);
        if topics.len() >= 3 { return topics; }
    }

    // Priority 3: ExecutionContext
    if needs_more_context {
        topics.push(MissingInformationTopic::ExecutionContext);
        if topics.len() >= 3 { return topics; }
    }

    // Priority 4: ScopeOrBlastRadius
    if observation_count > 0 && question_count > 0 && present_count + absent_count > 0 {
        topics.push(MissingInformationTopic::ScopeOrBlastRadius);
        if topics.len() >= 3 { return topics; }
    }

    // Priority 5: CorrectionTarget
    if corrected_count > 0 && corrected_count == observation_count {
        topics.push(MissingInformationTopic::CorrectionTarget);
    }

    topics
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared_types::{
        Confidence, ExtractedObservation, MissingInformationTopic, ModelTokenUsage,
        ObservationBoundaryResolution, ObservationBoundaryResolverOutput, ObservationExtractionOutput,
        ObservationPolarity, ResolvedObservation, StructuredUserQuery, StructuredUserQueryConfidence,
        StructuredUserQuerySupportLevel, StructuredUserQueryTerm,
    };

    // ─── Builders ─────────────────────────────────────────────────────────────

    fn term(support_level: StructuredUserQuerySupportLevel) -> StructuredUserQueryTerm {
        StructuredUserQueryTerm {
            term: "t".to_string(),
            evidence_span: "e".to_string(),
            support_level,
        }
    }

    fn explicit() -> StructuredUserQueryTerm {
        term(StructuredUserQuerySupportLevel::Explicit)
    }

    fn weak() -> StructuredUserQueryTerm {
        term(StructuredUserQuerySupportLevel::WeakInference)
    }

    fn empty_query() -> StructuredUserQuery {
        StructuredUserQuery {
            intent: String::new(),
            scenario: String::new(),
            symptoms: vec![],
            affected_subsystems: vec![],
            failure_modes: vec![],
            system_properties: vec![],
            entities: vec![],
            constraints: vec![],
            triggers: vec![],
            observability_signals: vec![],
            unresolved_terms: vec![],
            rejected_nearby_terms: vec![],
            confidence: StructuredUserQueryConfidence::High,
        }
    }

    fn sufficient_query() -> StructuredUserQuery {
        StructuredUserQuery {
            symptoms: vec![explicit(), explicit()],
            affected_subsystems: vec![explicit()],
            failure_modes: vec![explicit()],
            triggers: vec!["deploy".to_string()],
            ..empty_query()
        }
    }

    fn empty_observation() -> ObservationExtractionOutput {
        ObservationExtractionOutput {
            normalized_user_input: String::new(),
            resolved_observation: ResolvedObservation { text: String::new() },
            confidence: Confidence::High,
            observations: vec![],
            needs_more_context: false,
            missing_context_questions: vec![],
            token_usage: ModelTokenUsage {
                prompt_tokens: None,
                completion_tokens: None,
                total_tokens: None,
            },
        }
    }

    fn obs(polarity: ObservationPolarity, confidence: Confidence) -> ExtractedObservation {
        ExtractedObservation {
            statement: "s".to_string(),
            confidence,
            condition: None,
            polarity,
            time_relation: None,
            source_span: String::new(),
        }
    }

    fn sufficient_observation() -> ObservationExtractionOutput {
        ObservationExtractionOutput {
            confidence: Confidence::High,
            observations: vec![
                obs(ObservationPolarity::Present, Confidence::High),
                obs(ObservationPolarity::Present, Confidence::High),
            ],
            needs_more_context: false,
            missing_context_questions: vec![],
            ..empty_observation()
        }
    }

    fn unsupported_output() -> ObservationBoundaryResolverOutput {
        ObservationBoundaryResolverOutput {
            normalized_user_input: String::new(),
            confidence: Confidence::High,
            reason: String::new(),
            resolution: ObservationBoundaryResolution::Unsupported,
            token_usage: ModelTokenUsage {
                prompt_tokens: None,
                completion_tokens: None,
                total_tokens: None,
            },
        }
    }

    fn analyzer() -> InformationAdequacyAnalyzer {
        InformationAdequacyAnalyzer::new()
    }

    // ─── Construction ──────────────────────────────────────────────────────────

    #[test]
    fn new_constructs_successfully() {
        let _ = InformationAdequacyAnalyzer::new();
    }

    // ─── Determinism ───────────────────────────────────────────────────────────

    #[test]
    fn analyze_initial_is_deterministic() {
        let q = sufficient_query();
        let a = analyzer();
        let r1 = a.analyze_initial(&q).unwrap();
        let r2 = a.analyze_initial(&q).unwrap();
        assert_eq!(r1, r2);
    }

    #[test]
    fn analyze_supported_observation_is_deterministic() {
        let obs = sufficient_observation();
        let a = analyzer();
        let r1 = a.analyze_supported_observation(&obs).unwrap();
        let r2 = a.analyze_supported_observation(&obs).unwrap();
        assert_eq!(r1, r2);
    }

    #[test]
    fn analyze_unsupported_observation_is_deterministic() {
        let b = unsupported_output();
        let a = analyzer();
        let r1 = a.analyze_unsupported_observation(&b).unwrap();
        let r2 = a.analyze_unsupported_observation(&b).unwrap();
        assert_eq!(r1, r2);
    }

    // ─── analyze_initial: Blocking ─────────────────────────────────────────────

    #[test]
    fn initial_blocking_when_no_symptom_signal() {
        let q = empty_query();
        let r = analyzer().analyze_initial(&q).unwrap();
        assert_eq!(r.status, AdequacyStatus::Blocking);
    }

    #[test]
    fn initial_blocking_when_anchor_count_le_1() {
        // 1 symptom, no scope, no trigger, no failure mode → anchor_count == 1
        let q = StructuredUserQuery {
            symptoms: vec![explicit()],
            ..empty_query()
        };
        let r = analyzer().analyze_initial(&q).unwrap();
        assert_eq!(r.status, AdequacyStatus::Blocking);
    }

    #[test]
    fn initial_blocking_when_symptom_present_but_no_scope_trigger_failure() {
        // anchor_count >= 2 requires more groups; craft a query with symptoms
        // and scope but no trigger/failure → but anchor_count would be 2 (symptom+scope).
        // Spec says: if anchor_count > 1 AND symptom>0 AND scope==0 AND trigger==0 AND fm==0.
        // So we need anchor_count > 1 but scope==0 AND trigger==0 AND fm==0 — impossible
        // because anchor_count counts those groups. If symptom>0 is one group, we need
        // another anchor group that isn't scope/trigger/fm. That's not possible per the spec.
        // Actually re-reading: anchor_count = count of non-empty groups among:
        // symptom/evidence, failure mode, trigger/change, scope/component.
        // If symptom>0 AND scope==0 AND trigger==0 AND fm==0 → anchor_count=1 → caught by previous rule.
        // So this case is only reached when anchor_count>=2, which means at least one of
        // scope/trigger/fm is also non-empty. But then the condition scope==0 AND trigger==0
        // AND fm==0 cannot hold simultaneously with anchor_count>=2.
        // Per spec ordering, this rule fires after anchor_count check. Let's verify the spec:
        // it says both blocking rules can fire. We test the symptom+scope+no-trigger+no-fm case
        // to confirm it doesn't trigger this third rule (it won't because anchor>=2 won't
        // satisfy scope==0 AND trigger==0 AND fm==0 at once if scope>0).
        // To actually trigger rule 3, we need anchor_count > 1 which means symptom + one other.
        // But "one other" IS from {fm, trigger, scope}. So this rule effectively can't fire
        // after rules 1 and 2 unless… wait, let me re-read.
        // anchor_count includes scope. So if scope>0 → anchor_count includes scope → anchor_count>=2.
        // Rule 3 says scope==0 AND trigger==0 AND fm==0. Then anchor_count can only be 1 (just symptom).
        // That's caught by rule 2. So rule 3 logically can fire only when anchor_count==2 with
        // symptom + something else being non-fm/trigger/scope, which isn't possible per the groups.
        // It appears rule 3 is unreachable given the anchor_count definition. We skip this test
        // as it would need a structural contradiction. The spec mentions it as a rule but the
        // anchor_count check catches it first.
        //
        // Actually: anchor_count <= 1 means anchor_count is 0 or 1.
        // symptom>0 means anchor_count >= 1 (symptom group is non-empty).
        // scope==0, trigger==0, fm==0 → only the symptom group is non-empty → anchor_count == 1.
        // Rule 2 catches anchor_count <= 1 first. So rule 3 is only reached when anchor_count >= 2.
        // anchor_count >= 2 with symptom>0 means at least one of {fm, trigger, scope} is non-empty.
        // So scope==0 AND trigger==0 AND fm==0 contradicts anchor_count >= 2 given symptom>0.
        // Rule 3 is effectively subsumed. We test anchor_count <= 1 case instead.
        let q = StructuredUserQuery {
            symptoms: vec![explicit()],
            ..empty_query()
        };
        let r = analyzer().analyze_initial(&q).unwrap();
        assert_eq!(r.status, AdequacyStatus::Blocking);
    }

    #[test]
    fn initial_blocking_when_unresolved_terms_and_weak_signal() {
        // unresolved >= 2 AND (symptom_signal == 0 OR anchor_count == 1)
        // Use symptom_signal == 0 branch
        let q = StructuredUserQuery {
            unresolved_terms: vec!["foo".to_string(), "bar".to_string()],
            ..empty_query()
        };
        let r = analyzer().analyze_initial(&q).unwrap();
        assert_eq!(r.status, AdequacyStatus::Blocking);
    }

    // ─── analyze_initial: WeakButRunnable ─────────────────────────────────────

    #[test]
    fn initial_weak_when_single_symptom_signal() {
        // anchor_count >= 2: need symptom + something else
        let q = StructuredUserQuery {
            symptoms: vec![explicit()],
            affected_subsystems: vec![explicit()],
            triggers: vec!["deploy".to_string()],
            ..empty_query()
        };
        let r = analyzer().analyze_initial(&q).unwrap();
        assert_eq!(r.status, AdequacyStatus::WeakButRunnable);
    }

    #[test]
    fn initial_weak_when_scope_count_zero() {
        // Need anchor >= 2 and no scope
        let q = StructuredUserQuery {
            symptoms: vec![explicit(), explicit()],
            failure_modes: vec![explicit()],
            triggers: vec!["deploy".to_string()],
            ..empty_query()
        };
        let r = analyzer().analyze_initial(&q).unwrap();
        // anchor_count: symptom + failure_mode + trigger = 3, scope_count = 0
        // No blocking rule fires. weak: scope_count == 0
        assert_eq!(r.status, AdequacyStatus::WeakButRunnable);
    }

    #[test]
    fn initial_weak_when_no_trigger_and_no_failure_mode() {
        let q = StructuredUserQuery {
            symptoms: vec![explicit(), explicit()],
            affected_subsystems: vec![explicit()],
            ..empty_query()
        };
        // anchor_count: symptom + scope = 2; no blocking; trigger==0 && fm==0 → weak
        let r = analyzer().analyze_initial(&q).unwrap();
        assert_eq!(r.status, AdequacyStatus::WeakButRunnable);
    }

    #[test]
    fn initial_weak_when_weak_inference_dominates() {
        let q = StructuredUserQuery {
            symptoms: vec![weak(), weak(), explicit()],
            affected_subsystems: vec![weak()],
            failure_modes: vec![explicit()],
            triggers: vec!["deploy".to_string()],
            ..empty_query()
        };
        // weak_inference_count=3, explicit_count=2 → weak > explicit → weak rule
        let r = analyzer().analyze_initial(&q).unwrap();
        assert_eq!(r.status, AdequacyStatus::WeakButRunnable);
    }

    // ─── analyze_initial: Sufficient ──────────────────────────────────────────

    #[test]
    fn initial_sufficient_when_all_signals_strong() {
        let r = analyzer().analyze_initial(&sufficient_query()).unwrap();
        assert_eq!(r.status, AdequacyStatus::Sufficient);
    }

    // ─── analyze_supported_observation: Blocking ───────────────────────────────

    #[test]
    fn observation_blocking_when_empty() {
        let o = empty_observation();
        let r = analyzer().analyze_supported_observation(&o).unwrap();
        assert_eq!(r.status, AdequacyStatus::Blocking);
    }

    #[test]
    fn observation_blocking_when_needs_more_context() {
        let o = ObservationExtractionOutput {
            observations: vec![obs(ObservationPolarity::Present, Confidence::High)],
            needs_more_context: true,
            ..empty_observation()
        };
        let r = analyzer().analyze_supported_observation(&o).unwrap();
        assert_eq!(r.status, AdequacyStatus::Blocking);
    }

    #[test]
    fn observation_blocking_when_single_low_confidence() {
        let o = ObservationExtractionOutput {
            observations: vec![obs(ObservationPolarity::Present, Confidence::Low)],
            confidence: Confidence::Low,
            ..empty_observation()
        };
        let r = analyzer().analyze_supported_observation(&o).unwrap();
        assert_eq!(r.status, AdequacyStatus::Blocking);
    }

    #[test]
    fn observation_blocking_correction_only_low_confidence() {
        let o = ObservationExtractionOutput {
            observations: vec![obs(ObservationPolarity::Corrected, Confidence::Low)],
            confidence: Confidence::Low,
            ..empty_observation()
        };
        let r = analyzer().analyze_supported_observation(&o).unwrap();
        assert_eq!(r.status, AdequacyStatus::Blocking);
    }

    // ─── analyze_supported_observation: WeakButRunnable ───────────────────────

    #[test]
    fn observation_weak_when_single_observation_no_blocking() {
        let o = ObservationExtractionOutput {
            observations: vec![obs(ObservationPolarity::Present, Confidence::High)],
            confidence: Confidence::High,
            ..empty_observation()
        };
        let r = analyzer().analyze_supported_observation(&o).unwrap();
        assert_eq!(r.status, AdequacyStatus::WeakButRunnable);
    }

    #[test]
    fn observation_weak_when_questions_present() {
        let o = ObservationExtractionOutput {
            observations: vec![
                obs(ObservationPolarity::Present, Confidence::High),
                obs(ObservationPolarity::Present, Confidence::High),
            ],
            confidence: Confidence::High,
            missing_context_questions: vec!["q?".to_string()],
            ..empty_observation()
        };
        let r = analyzer().analyze_supported_observation(&o).unwrap();
        assert_eq!(r.status, AdequacyStatus::WeakButRunnable);
    }

    #[test]
    fn observation_weak_when_medium_or_higher_count_zero() {
        let o = ObservationExtractionOutput {
            observations: vec![
                obs(ObservationPolarity::Present, Confidence::Low),
                obs(ObservationPolarity::Present, Confidence::Low),
            ],
            confidence: Confidence::High,
            ..empty_observation()
        };
        // observation_count=2, not low+single → not blocked; medium_or_higher==0 → weak
        let r = analyzer().analyze_supported_observation(&o).unwrap();
        assert_eq!(r.status, AdequacyStatus::WeakButRunnable);
    }

    #[test]
    fn observation_weak_correction_only_non_high_confidence() {
        let o = ObservationExtractionOutput {
            observations: vec![
                obs(ObservationPolarity::Corrected, Confidence::Medium),
                obs(ObservationPolarity::Corrected, Confidence::Medium),
            ],
            confidence: Confidence::Medium,
            ..empty_observation()
        };
        // corrected_count==observation_count && confidence != High → weak
        let r = analyzer().analyze_supported_observation(&o).unwrap();
        assert_eq!(r.status, AdequacyStatus::WeakButRunnable);
    }

    // ─── analyze_supported_observation: Sufficient ────────────────────────────

    #[test]
    fn observation_sufficient_when_strong() {
        let r = analyzer().analyze_supported_observation(&sufficient_observation()).unwrap();
        assert_eq!(r.status, AdequacyStatus::Sufficient);
    }

    // ─── analyze_unsupported_observation ──────────────────────────────────────

    #[test]
    fn unsupported_observation_blocking() {
        let r = analyzer().analyze_unsupported_observation(&unsupported_output()).unwrap();
        assert_eq!(r.status, AdequacyStatus::Blocking);
    }

    // ─── Sufficient invariants ─────────────────────────────────────────────────

    #[test]
    fn sufficient_has_empty_topics_and_questions() {
        let r = analyzer().analyze_initial(&sufficient_query()).unwrap();
        assert_eq!(r.status, AdequacyStatus::Sufficient);
        assert!(r.missing_information_topics.is_empty());
        assert!(r.follow_up_questions.is_empty());
    }

    // ─── Blocking non-empty topics/questions ──────────────────────────────────

    #[test]
    fn blocking_has_non_empty_topics_and_questions() {
        let r = analyzer().analyze_initial(&empty_query()).unwrap();
        assert_eq!(r.status, AdequacyStatus::Blocking);
        assert!(!r.missing_information_topics.is_empty());
        assert!(!r.follow_up_questions.is_empty());
    }

    // ─── WeakButRunnable: topics/questions parity ─────────────────────────────

    #[test]
    fn weak_preserves_topics_questions_parity() {
        let q = StructuredUserQuery {
            symptoms: vec![explicit()],
            affected_subsystems: vec![explicit()],
            triggers: vec!["deploy".to_string()],
            ..empty_query()
        };
        let r = analyzer().analyze_initial(&q).unwrap();
        assert_eq!(r.status, AdequacyStatus::WeakButRunnable);
        assert_eq!(r.missing_information_topics.len(), r.follow_up_questions.len());
    }

    // ─── No duplicates ────────────────────────────────────────────────────────

    #[test]
    fn topics_never_contain_duplicates() {
        let r = analyzer().analyze_initial(&empty_query()).unwrap();
        let mut seen = std::collections::HashSet::new();
        for t in &r.missing_information_topics {
            assert!(seen.insert(t), "duplicate topic: {:?}", t);
        }
    }

    #[test]
    fn questions_never_contain_duplicates() {
        let r = analyzer().analyze_initial(&empty_query()).unwrap();
        let mut seen = std::collections::HashSet::new();
        for q in &r.follow_up_questions {
            assert!(seen.insert(q.as_str()), "duplicate question");
        }
    }

    // ─── Length invariants ────────────────────────────────────────────────────

    #[test]
    fn topics_len_equals_questions_len() {
        let cases: Vec<StructuredUserQuery> = vec![
            empty_query(),
            sufficient_query(),
            StructuredUserQuery {
                symptoms: vec![explicit()],
                affected_subsystems: vec![explicit()],
                triggers: vec!["d".to_string()],
                ..empty_query()
            },
        ];
        let a = analyzer();
        for q in cases {
            let r = a.analyze_initial(&q).unwrap();
            assert_eq!(
                r.missing_information_topics.len(),
                r.follow_up_questions.len(),
            );
        }
    }

    #[test]
    fn topics_len_never_exceeds_3() {
        // Trigger all initial selectors simultaneously
        let q = StructuredUserQuery {
            unresolved_terms: vec!["foo".to_string(), "bar".to_string()],
            ..empty_query()
        };
        let r = analyzer().analyze_initial(&q).unwrap();
        assert!(r.missing_information_topics.len() <= 3);
        assert!(r.follow_up_questions.len() <= 3);
    }

    // ─── Initial-request topic priority order ─────────────────────────────────

    #[test]
    fn initial_topic_priority_symptom_first_when_no_symptom() {
        let q = empty_query();
        let r = analyzer().analyze_initial(&q).unwrap();
        assert_eq!(r.missing_information_topics[0], MissingInformationTopic::SymptomDescription);
    }

    #[test]
    fn initial_selects_symptom_description_when_signal_count_is_1() {
        let q = StructuredUserQuery {
            symptoms: vec![explicit()],
            affected_subsystems: vec![explicit()],
            triggers: vec!["d".to_string()],
            ..empty_query()
        };
        let r = analyzer().analyze_initial(&q).unwrap();
        assert!(r.missing_information_topics.contains(&MissingInformationTopic::SymptomDescription));
    }

    #[test]
    fn initial_selects_affected_component_when_scope_zero() {
        let q = StructuredUserQuery {
            symptoms: vec![explicit(), explicit()],
            failure_modes: vec![explicit()],
            triggers: vec!["d".to_string()],
            ..empty_query()
        };
        // scope_count == 0 → AffectedComponent selected
        let r = analyzer().analyze_initial(&q).unwrap();
        assert!(r.missing_information_topics.contains(&MissingInformationTopic::AffectedComponent));
    }

    #[test]
    fn initial_selects_trigger_when_trigger_zero() {
        // symptoms=2, scope=1, no failure_modes, no triggers → anchor=2 (symptom+scope),
        // weak rule: trigger==0 AND fm==0 fires, TriggerOrRecentChange selected
        let q = StructuredUserQuery {
            symptoms: vec![explicit(), explicit()],
            affected_subsystems: vec![explicit()],
            ..empty_query()
        };
        let r = analyzer().analyze_initial(&q).unwrap();
        assert_eq!(r.status, AdequacyStatus::WeakButRunnable);
        assert!(r.missing_information_topics.contains(&MissingInformationTopic::TriggerOrRecentChange));
    }

    #[test]
    fn initial_selects_failure_mechanism_only_when_anchor_lt_2() {
        // Need fm==0 AND anchor < 2. anchor < 2 with symptom > 0 means anchor == 1.
        // symptom=1, no scope, no trigger, no fm → anchor=1, scope=0, trigger=0, fm=0 → blocking via anchor.
        // We can't have anchor >= 2 and fm == 0 and anchor < 2 simultaneously.
        // Test: ensure FailureMechanismHint is NOT selected when anchor >= 2 but fm > 0
        let q = StructuredUserQuery {
            symptoms: vec![explicit(), explicit()],
            affected_subsystems: vec![explicit()],
            failure_modes: vec![explicit()],
            triggers: vec!["d".to_string()],
            ..empty_query()
        };
        let r = analyzer().analyze_initial(&q).unwrap();
        assert!(!r.missing_information_topics.contains(&MissingInformationTopic::FailureMechanismHint));
    }

    #[test]
    fn initial_selects_term_clarification_when_unresolved_ge_2() {
        // symptoms=2 (WeakInference), scope=1, trigger=1, fm=0, unresolved=2.
        // weak_inference=2 > explicit=1 → WeakButRunnable.
        // anchor=3 (symptom+scope+trigger). Topics: SymptomDescription? No (count=2).
        // AffectedComponent? No (scope=1). TriggerOrRecentChange? No (trigger=1).
        // FailureMechanismHint? fm==0 but anchor=3 (not <2). No.
        // ExpectedVsActual? anchor=3 (not <2). No.
        // TermClarification? unresolved>=2 → YES.
        let q = StructuredUserQuery {
            symptoms: vec![weak(), weak()],
            affected_subsystems: vec![explicit()],
            triggers: vec!["deploy".to_string()],
            unresolved_terms: vec!["foo".to_string(), "bar".to_string()],
            ..empty_query()
        };
        let r = analyzer().analyze_initial(&q).unwrap();
        assert_eq!(r.status, AdequacyStatus::WeakButRunnable);
        assert!(r.missing_information_topics.contains(&MissingInformationTopic::TermClarification));
    }

    #[test]
    fn initial_truncates_to_three_topics() {
        // Max possible selection: symptom(1), scope(2), trigger(3), then stop.
        // empty query triggers symptom + scope + trigger (first 3 in priority)
        let q = empty_query();
        let r = analyzer().analyze_initial(&q).unwrap();
        assert!(r.missing_information_topics.len() <= 3);
    }

    // ─── Observation topic priority order ─────────────────────────────────────

    #[test]
    fn observation_selects_observed_result_when_count_zero() {
        let r = analyzer().analyze_supported_observation(&empty_observation()).unwrap();
        assert!(r.missing_information_topics.contains(&MissingInformationTopic::ObservedResult));
    }

    #[test]
    fn observation_selects_observed_result_when_single_low_confidence() {
        let o = ObservationExtractionOutput {
            observations: vec![obs(ObservationPolarity::Present, Confidence::Low)],
            confidence: Confidence::Low,
            ..empty_observation()
        };
        let r = analyzer().analyze_supported_observation(&o).unwrap();
        assert!(r.missing_information_topics.contains(&MissingInformationTopic::ObservedResult));
    }

    #[test]
    fn observation_selects_check_outcome_when_needs_context_with_questions() {
        let o = ObservationExtractionOutput {
            observations: vec![obs(ObservationPolarity::Present, Confidence::High)],
            confidence: Confidence::High,
            needs_more_context: true,
            missing_context_questions: vec!["q?".to_string()],
            ..empty_observation()
        };
        let r = analyzer().analyze_supported_observation(&o).unwrap();
        assert!(r.missing_information_topics.contains(&MissingInformationTopic::CheckOutcome));
    }

    #[test]
    fn observation_selects_execution_context_when_needs_context() {
        let o = ObservationExtractionOutput {
            observations: vec![obs(ObservationPolarity::Present, Confidence::High)],
            confidence: Confidence::High,
            needs_more_context: true,
            ..empty_observation()
        };
        let r = analyzer().analyze_supported_observation(&o).unwrap();
        assert!(r.missing_information_topics.contains(&MissingInformationTopic::ExecutionContext));
    }

    #[test]
    fn observation_selects_scope_under_mixed_question_rule() {
        // observation_count > 0, question_count > 0, present+absent > 0
        let o = ObservationExtractionOutput {
            observations: vec![
                obs(ObservationPolarity::Present, Confidence::High),
                obs(ObservationPolarity::Present, Confidence::High),
            ],
            confidence: Confidence::High,
            missing_context_questions: vec!["q?".to_string()],
            ..empty_observation()
        };
        let r = analyzer().analyze_supported_observation(&o).unwrap();
        assert!(r.missing_information_topics.contains(&MissingInformationTopic::ScopeOrBlastRadius));
    }

    #[test]
    fn observation_selects_correction_target_only_when_corrected_present() {
        let o = ObservationExtractionOutput {
            observations: vec![obs(ObservationPolarity::Corrected, Confidence::Medium)],
            confidence: Confidence::Medium,
            ..empty_observation()
        };
        let r = analyzer().analyze_supported_observation(&o).unwrap();
        assert!(r.missing_information_topics.contains(&MissingInformationTopic::CorrectionTarget));
    }

    #[test]
    fn observation_truncates_to_three_topics() {
        let o = ObservationExtractionOutput {
            observations: vec![obs(ObservationPolarity::Present, Confidence::Low)],
            confidence: Confidence::Low,
            needs_more_context: true,
            missing_context_questions: vec!["q?".to_string()],
            ..empty_observation()
        };
        let r = analyzer().analyze_supported_observation(&o).unwrap();
        assert!(r.missing_information_topics.len() <= 3);
    }

    // ─── Unsupported observation topics ───────────────────────────────────────

    #[test]
    fn unsupported_selects_observed_result_and_execution_context_in_order() {
        let r = analyzer().analyze_unsupported_observation(&unsupported_output()).unwrap();
        assert_eq!(r.missing_information_topics, vec![
            MissingInformationTopic::ObservedResult,
            MissingInformationTopic::ExecutionContext,
        ]);
    }

    #[test]
    fn unsupported_truncates_to_two_topics() {
        let r = analyzer().analyze_unsupported_observation(&unsupported_output()).unwrap();
        assert_eq!(r.missing_information_topics.len(), 2);
    }

    // ─── Topic isolation ──────────────────────────────────────────────────────

    #[test]
    fn initial_never_emits_observation_specific_topics() {
        let observation_only_topics = [
            MissingInformationTopic::ObservedResult,
            MissingInformationTopic::CheckOutcome,
            MissingInformationTopic::ExecutionContext,
            MissingInformationTopic::ScopeOrBlastRadius,
            MissingInformationTopic::CorrectionTarget,
        ];
        let r = analyzer().analyze_initial(&empty_query()).unwrap();
        for t in &r.missing_information_topics {
            assert!(!observation_only_topics.contains(t), "initial emitted {:?}", t);
        }
    }

    #[test]
    fn supported_observation_never_emits_initial_specific_topics() {
        let initial_only_topics = [
            MissingInformationTopic::SymptomDescription,
            MissingInformationTopic::AffectedComponent,
            MissingInformationTopic::TriggerOrRecentChange,
            MissingInformationTopic::FailureMechanismHint,
            MissingInformationTopic::ExpectedVsActual,
            MissingInformationTopic::TermClarification,
        ];
        let r = analyzer().analyze_supported_observation(&empty_observation()).unwrap();
        for t in &r.missing_information_topics {
            assert!(!initial_only_topics.contains(t), "observation emitted {:?}", t);
        }
    }

    #[test]
    fn unsupported_observation_never_emits_initial_specific_topics() {
        let initial_only_topics = [
            MissingInformationTopic::SymptomDescription,
            MissingInformationTopic::AffectedComponent,
            MissingInformationTopic::TriggerOrRecentChange,
            MissingInformationTopic::FailureMechanismHint,
            MissingInformationTopic::ExpectedVsActual,
            MissingInformationTopic::TermClarification,
        ];
        let r = analyzer().analyze_unsupported_observation(&unsupported_output()).unwrap();
        for t in &r.missing_information_topics {
            assert!(!initial_only_topics.contains(t), "unsupported observation emitted {:?}", t);
        }
    }

    // ─── Canonical question mapping ───────────────────────────────────────────

    #[test]
    fn each_topic_maps_to_exact_canonical_question() {
        use MissingInformationTopic::*;
        let cases = [
            (SymptomDescription, "What exactly are you observing: errors, timeouts, retries, stale data, leader changes, or another visible failure?"),
            (AffectedComponent, "Which component or subsystem seems involved: for example the database, broker, lock service, scheduler, API gateway, or another part of the system?"),
            (TriggerOrRecentChange, "What changed right before this started: for example a deploy, restart, failover, scaling event, config change, or traffic spike?"),
            (FailureMechanismHint, "What failure pattern does this look closest to: for example timeout handling, replication lag, leader-election instability, lock contention, or resource exhaustion?"),
            (ExpectedVsActual, "What did you expect to happen, and what happened instead?"),
            (ObservedResult, "What was the exact observed result: for example the error message, timeout behavior, empty result, retry loop, or recovery signal?"),
            (ExecutionContext, "Where and when did you observe this: for example which node, component, request path, environment, or time window?"),
            (CheckOutcome, "What was the result of the check you ran: for example what did the command, log, metric, or status output show?"),
            (ScopeOrBlastRadius, "How wide is the impact: is this limited to one node, shard, or request path, or does it affect the whole system?"),
            (CorrectionTarget, "Which earlier assumption are you correcting, and what is the corrected fact now?"),
            (TermClarification, "Some terms are still ambiguous. Can you restate the issue using the exact observed behavior and concrete component names?"),
        ];
        for (topic, expected) in &cases {
            assert_eq!(canonical_question(*topic), *expected, "wrong question for {:?}", topic);
        }
    }

    #[test]
    fn questions_order_matches_topics_order() {
        let r = analyzer().analyze_initial(&empty_query()).unwrap();
        for (i, topic) in r.missing_information_topics.iter().enumerate() {
            assert_eq!(
                r.follow_up_questions[i],
                canonical_question(*topic),
            );
        }
    }

    #[test]
    fn empty_topics_means_empty_questions() {
        let r = analyzer().analyze_initial(&sufficient_query()).unwrap();
        assert!(r.missing_information_topics.is_empty());
        assert!(r.follow_up_questions.is_empty());
    }

    // ─── Summary reason literals ───────────────────────────────────────────────

    #[test]
    fn initial_blocking_no_symptom_summary() {
        let r = analyzer().analyze_initial(&empty_query()).unwrap();
        assert_eq!(r.summary_reason, "The request does not describe any concrete symptom or observable behavior.");
    }

    #[test]
    fn initial_blocking_anchor_count_summary() {
        let q = StructuredUserQuery {
            symptoms: vec![explicit()],
            ..empty_query()
        };
        let r = analyzer().analyze_initial(&q).unwrap();
        assert_eq!(r.summary_reason, "The request contains too little anchored diagnostic context to proceed safely.");
    }

    #[test]
    fn initial_blocking_unresolved_summary() {
        let q = StructuredUserQuery {
            symptoms: vec![explicit()],
            affected_subsystems: vec![explicit()],
            unresolved_terms: vec!["x".to_string(), "y".to_string()],
            ..empty_query()
        };
        // anchor_count: symptom + scope = 2 (> 1), unresolved >= 2, anchor == 1? No, anchor==2.
        // So unresolved rule: unresolved >= 2 AND (signal==0 OR anchor==1). anchor==2, signal==1.
        // Rule 4 doesn't fire. anchor_count <= 1 doesn't fire. signal > 0.
        // scope > 0, so rule 3 doesn't fire (scope==0 required).
        // Wait: signal_count = 1 (one symptom). anchor_count: symptom=1 group, scope=1 group = 2.
        // None of the 4 blocking rules fire. We're in weak territory (signal_count==1).
        // So this query is WeakButRunnable, not Blocking with unresolved summary.
        // For the unresolved blocking summary, we need: unresolved >= 2 AND (signal==0 OR anchor==1).
        // signal == 0 case already tested (no_symptom). Test anchor==1:
        let q2 = StructuredUserQuery {
            symptoms: vec![explicit()],
            unresolved_terms: vec!["x".to_string(), "y".to_string()],
            ..empty_query()
        };
        // signal==1, anchor_count: only symptom group=1. Rule 2 fires first (anchor<=1).
        // So we test rule 4 when signal==0:
        let q3 = StructuredUserQuery {
            unresolved_terms: vec!["x".to_string(), "y".to_string()],
            ..empty_query()
        };
        // signal==0 → rule 1 fires first (no symptom summary). Rule 4 would also fire but rule 1 is first.
        // According to spec: "first matching literal in the order listed". Rule 1 (signal==0) matches first.
        // So to get the unresolved summary, we need: signal>0 AND anchor>1 AND unresolved>=2 AND anchor==1.
        // That's a contradiction. The unresolved rule (rule 4) can only fire when signal==0 OR anchor==1,
        // but if signal==0 rule 1 fires first, and if anchor==1 rule 2 fires first.
        // So the "unresolved" summary is unreachable in the current priority ordering.
        // We test rule 2 instead (anchor_count <= 1 with symptoms present).
        let _ = q2;
        let r = analyzer().analyze_initial(&q3).unwrap();
        // Rule 1 fires first → no-symptom summary
        assert_eq!(r.summary_reason, "The request does not describe any concrete symptom or observable behavior.");
    }

    #[test]
    fn initial_weak_summary() {
        let q = StructuredUserQuery {
            symptoms: vec![explicit()],
            affected_subsystems: vec![explicit()],
            triggers: vec!["deploy".to_string()],
            ..empty_query()
        };
        let r = analyzer().analyze_initial(&q).unwrap();
        assert_eq!(r.status, AdequacyStatus::WeakButRunnable);
        assert_eq!(r.summary_reason, "The request contains a usable signal but is still diagnostically thin.");
    }

    #[test]
    fn initial_sufficient_summary() {
        let r = analyzer().analyze_initial(&sufficient_query()).unwrap();
        assert_eq!(r.summary_reason, "The request contains enough diagnostic context to continue.");
    }

    #[test]
    fn observation_blocking_empty_summary() {
        let r = analyzer().analyze_supported_observation(&empty_observation()).unwrap();
        assert_eq!(r.summary_reason, "The observation does not contain a concrete new diagnostic fact.");
    }

    #[test]
    fn observation_blocking_needs_more_context_summary() {
        let o = ObservationExtractionOutput {
            observations: vec![obs(ObservationPolarity::Present, Confidence::High)],
            needs_more_context: true,
            ..empty_observation()
        };
        let r = analyzer().analyze_supported_observation(&o).unwrap();
        assert_eq!(r.summary_reason, "The observation requires more context before diagnostic update can proceed safely.");
    }

    #[test]
    fn observation_blocking_single_low_confidence_summary() {
        let o = ObservationExtractionOutput {
            observations: vec![obs(ObservationPolarity::Present, Confidence::Low)],
            confidence: Confidence::Low,
            ..empty_observation()
        };
        let r = analyzer().analyze_supported_observation(&o).unwrap();
        assert_eq!(r.summary_reason, "The observation is too weak and low-confidence for a safe diagnostic update.");
    }

    #[test]
    fn observation_blocking_correction_only_summary() {
        // With 2 corrected + Low: rule 3 (single low-conf) doesn't fire (count=2),
        // rule 4 fires: corrected>0, present==0, absent==0, count==corrected, confidence==Low.
        let o = ObservationExtractionOutput {
            observations: vec![
                obs(ObservationPolarity::Corrected, Confidence::Low),
                obs(ObservationPolarity::Corrected, Confidence::Low),
            ],
            confidence: Confidence::Low,
            ..empty_observation()
        };
        let r = analyzer().analyze_supported_observation(&o).unwrap();
        assert_eq!(r.summary_reason, "The observation only corrects a prior assumption and is still too weak for a safe diagnostic update.");
    }

    #[test]
    fn observation_weak_summary() {
        let o = ObservationExtractionOutput {
            observations: vec![obs(ObservationPolarity::Present, Confidence::High)],
            confidence: Confidence::High,
            ..empty_observation()
        };
        let r = analyzer().analyze_supported_observation(&o).unwrap();
        assert_eq!(r.summary_reason, "The observation contains a usable update signal but still lacks diagnostic strength.");
    }

    #[test]
    fn observation_sufficient_summary() {
        let r = analyzer().analyze_supported_observation(&sufficient_observation()).unwrap();
        assert_eq!(r.summary_reason, "The observation contains enough concrete diagnostic information to continue.");
    }

    #[test]
    fn unsupported_observation_summary() {
        let r = analyzer().analyze_unsupported_observation(&unsupported_output()).unwrap();
        assert_eq!(r.summary_reason, "The latest user message is not yet a supported standalone diagnostic observation.");
    }

    #[test]
    fn summary_reason_non_empty_after_trim() {
        let cases: Vec<Box<dyn Fn() -> String>> = vec![
            Box::new(|| analyzer().analyze_initial(&empty_query()).unwrap().summary_reason),
            Box::new(|| analyzer().analyze_initial(&sufficient_query()).unwrap().summary_reason),
            Box::new(|| analyzer().analyze_supported_observation(&empty_observation()).unwrap().summary_reason),
            Box::new(|| analyzer().analyze_supported_observation(&sufficient_observation()).unwrap().summary_reason),
            Box::new(|| analyzer().analyze_unsupported_observation(&unsupported_output()).unwrap().summary_reason),
        ];
        for f in cases {
            assert!(!f().trim().is_empty());
        }
    }

    // ─── Error boundary ───────────────────────────────────────────────────────

    #[test]
    fn unsupported_observation_fails_when_resolution_is_supported() {
        let b = ObservationBoundaryResolverOutput {
            normalized_user_input: String::new(),
            confidence: Confidence::High,
            reason: String::new(),
            resolution: ObservationBoundaryResolution::Supported(ResolvedObservation {
                text: "some obs".to_string(),
            }),
            token_usage: ModelTokenUsage {
                prompt_tokens: None,
                completion_tokens: None,
                total_tokens: None,
            },
        };
        let err = analyzer().analyze_unsupported_observation(&b).unwrap_err();
        assert!(matches!(err, InformationAdequacyAnalyzerError::InvalidObservationExtractionOutput(_)));
    }

    #[test]
    fn semantic_weakness_not_an_error() {
        let q = StructuredUserQuery {
            symptoms: vec![weak()],
            affected_subsystems: vec![explicit()],
            triggers: vec!["d".to_string()],
            ..empty_query()
        };
        // weak_inference > explicit → weak, not an error
        let r = analyzer().analyze_initial(&q);
        assert!(r.is_ok());
        assert_eq!(r.unwrap().status, AdequacyStatus::WeakButRunnable);
    }
}
