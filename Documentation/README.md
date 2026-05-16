# Documentation Index

This folder contains the main narrative documentation for the runtime, reasoning model, and evaluation story of the project.

These documents explain how the diagnostic assistant works as an engineering system: what problem it solves, how the runtime pipeline is structured, how continuation works, how prompt context is assembled, and how the project is evaluated and observed.

They complement the repository's implementation and specifications, but they do not replace them.

For repository-local working conventions for coding agents, see [../AGENTS.md](../AGENTS.md).

* * *
## Recommended Reading Path

If you want the fastest high-level understanding of the project, read the documents in this order:

1. [Overview](./OVERVIEW.md)  
   Product framing and the central diagnostic idea.

2. [Diagnostic Model](./DIAGNOSTIC_MODEL.md)  
   The reasoning state exposed to the user: hypotheses, competing interpretation, and discriminating check.

3. [Case Study: Amazon RDS Reader Stale Reads](./CASE_STUDY_AMAZON_RDS_READER_STALE_READS.md)  
   A concrete three-iteration walkthrough of the runtime on one incident.

4. [Evaluation Story](./EVALUATION_STORY.md)  
   How runtime behavior is judged and compared across runs.

5. [Observability Story](./OBSERVABILITY_STORY.md)  
   How traces and other runtime signals support debugging and inspection.

6. [Architecture](./ARCHITECTURE.md)  
   How the runtime pipeline implements the reasoning model.

## For Deeper Understanding

After the quickstart path above, continue with:

7. [Key Engineering Decisions](./KEY_ENGINEERING_DECISIONS.md)  
   The main design choices, tradeoffs, and rationale behind the project shape.

8. [Specification-First Approach](./SPECIFICATION_FIRST_APPROACH.md)  
   Why specifications are treated as the design authority for generation, tests, and review.

9. [Repository Map](./REPOSITORY_MAP.md)  
   How the repository is split across execution, specification, measurement, evidence, and documentation.

10. [Prompt Context Assembly](./PROMPT_CONTEXT_ASSEMBLY.md)  
    What evidence enters the model context and how it is role-packed.

* * *
## Core Documents

### [OVERVIEW.md](./OVERVIEW.md)

The shortest product-level introduction to the project.

Explains what makes the assistant different from generic RAG, why it is organized around a diagnostic state rather than a one-shot answer, and how the continuation loop changes the investigation over time.

### [ARCHITECTURE.md](./ARCHITECTURE.md)

The main runtime architecture document.

Describes the orchestrator, request pipeline, run and iteration model, storage boundaries, and the split between control flow and step execution.

### [DIAGNOSTIC_MODEL.md](./DIAGNOSTIC_MODEL.md)

Defines the reasoning model used by the assistant.

Explains the role of `problem_understanding`, hypotheses, competing interpretation, discriminating check, and how those fields should evolve across iterations.

### [KEY_ENGINEERING_DECISIONS.md](./KEY_ENGINEERING_DECISIONS.md)

Explains why the project is shaped the way it is.

Captures the main architectural and engineering decisions, including orchestration boundaries, evidence roles, evaluation structure, and shared runtime/eval contracts.

### [SPECIFICATION_FIRST_APPROACH.md](./SPECIFICATION_FIRST_APPROACH.md)

Explains why the repository treats specifications as the source of truth.

Describes how specs, generation, tests, and review are related in this project.

### [REPOSITORY_MAP.md](./REPOSITORY_MAP.md)

Explains the repository layout in clear project terms.

Useful for onboarding, presentation, and understanding why the top-level structure is intentionally split across distinct engineering surfaces.

### [PROMPT_CONTEXT_ASSEMBLY.md](./PROMPT_CONTEXT_ASSEMBLY.md)

Explains what is sent to the model before generation.

Documents the prompt-facing context blocks, role-based chunk packing, and the difference between initial and continuation prompt assembly.

### [CASE_STUDY_AMAZON_RDS_READER_STALE_READS.md](./CASE_STUDY_AMAZON_RDS_READER_STALE_READS.md)

An end-to-end narrative example of the system in action.

Shows how the first diagnostic frame is built, how later observations change the frame, and how the competing explanations and next check evolve across three iterations.

### [EVALUATION_STORY.md](./EVALUATION_STORY.md)

Describes how the project evaluates runtime quality.

Useful for understanding what "good behavior" means in this repository and how first-response and continuation quality are judged.

### [OBSERVABILITY_STORY.md](./OBSERVABILITY_STORY.md)

Explains how the project is inspected in operation.

Focuses on traces and the observability surfaces used to understand failures, weak answers, and runtime behavior.

## Diagrams And Visual Assets

### [INITIAL_PIPELINE_OVERVIEW.uml](./INITIAL_PIPELINE_OVERVIEW.uml)

PlantUML source for the initial and continuation pipeline overview diagram.

### [INITIAL_PIPELINE_OVERVIEW.svg](./INITIAL_PIPELINE_OVERVIEW.svg)

Rendered pipeline overview diagram.

### [RUNTIME_ARCHITECTURE_OVERVIEW.uml](./RUNTIME_ARCHITECTURE_OVERVIEW.uml)

PlantUML source for the runtime architecture diagram.

### [RUNTIME_ARCHITECTURE_OVERVIEW.svg](./RUNTIME_ARCHITECTURE_OVERVIEW.svg)

Rendered runtime architecture diagram.

### [orchestration_run_state.uml](./orchestration_run_state.uml)

PlantUML source for the run-state structure diagram.

### [orchestration_run_state.svg](./orchestration_run_state.svg)

Rendered run-state structure diagram.

### [images/](./images)

Supporting screenshots and visual assets used by the docs.

* * *
## Trace Artifacts

### [case study traces/](./case%20study%20traces)

Trace exports referenced by the Amazon RDS case study.

These files are supporting evidence for the narrative walkthrough, not primary architecture documents.

* * *
## How To Use This Folder

A practical reading flow is:

- start with the product-level story in `OVERVIEW.md`;
- move to `DIAGNOSTIC_MODEL.md` to understand the reasoning model;
- use the case study to connect the abstractions to one concrete runtime run;
- finish with evaluation and observability if you want to inspect quality and runtime behavior in more depth;
- then read `ARCHITECTURE.md` and supporting docs for the implementation details.

If you arrived here from the repository root, this file is the best entry point for the documentation set.
