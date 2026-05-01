from __future__ import annotations
from contextlib import contextmanager
from typing import Iterator

from opentelemetry import trace
from opentelemetry.exporter.otlp.proto.grpc.trace_exporter import OTLPSpanExporter
from opentelemetry.sdk.resources import Resource
from opentelemetry.sdk.trace import TracerProvider
from opentelemetry.sdk.trace.export import SimpleSpanProcessor
from opentelemetry.trace import Status, StatusCode, use_span


_TRACER_PROVIDER: TracerProvider | None = None


def init_eval_tracing(
    service_name: str,
    endpoint: str,
) -> None:
    global _TRACER_PROVIDER
    if _TRACER_PROVIDER is not None:
        return

    resource = Resource.create({"service.name": service_name})
    provider = TracerProvider(resource=resource)
    exporter = OTLPSpanExporter(endpoint=endpoint, insecure=True)
    # Eval runs are slow and short-lived; synchronous export makes span lifecycle
    # more predictable than a batched background processor.
    provider.add_span_processor(SimpleSpanProcessor(exporter))
    trace.set_tracer_provider(provider)
    _TRACER_PROVIDER = provider


def shutdown_eval_tracing() -> None:
    provider = _TRACER_PROVIDER
    if provider is None:
        return

    force_flush = getattr(provider, "force_flush", None)
    if callable(force_flush):
        force_flush()

    shutdown = getattr(provider, "shutdown", None)
    if callable(shutdown):
        shutdown()


def get_tracer(name: str):
    return trace.get_tracer(name)


def span_context_for_child(span: object):
    return trace.set_span_in_context(span)


@contextmanager
def traced_operation(
    tracer_name: str,
    span_name: str,
    attributes: dict[str, object],
    parent_context: object | None = None,
) -> Iterator[object]:
    tracer = get_tracer(tracer_name)
    span = tracer.start_span(span_name, context=parent_context)
    for key, value in attributes.items():
        span.set_attribute(key, value)
    with use_span(span, end_on_exit=False):
        try:
            yield span
        except Exception as error:
            span.set_status(Status(StatusCode.ERROR, str(error)))
            span.record_exception(error)
            raise
        else:
            span.set_status(Status(StatusCode.OK))
        finally:
            span.end()
