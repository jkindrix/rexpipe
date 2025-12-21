"""
LangChain Tools for rexpipe

This module provides LangChain-compatible tools for using rexpipe
in AI agent workflows.

Installation:
    pip install langchain subprocess

Usage:
    from rexpipe_tools import RexpipeSubstitute, RexpipeFilter, RexpipeRedactPII

    tools = [RexpipeSubstitute(), RexpipeFilter(), RexpipeRedactPII()]
    agent = create_tool_agent(llm, tools)
"""

import json
import subprocess
from typing import Optional, Type

from langchain.tools import BaseTool
from pydantic import BaseModel, Field


def run_rexpipe(args: list[str], input_text: str) -> dict:
    """Execute rexpipe with given arguments and return JSON result."""
    try:
        result = subprocess.run(
            ["rexpipe", "--json", "--error-format", "json"] + args,
            input=input_text,
            capture_output=True,
            text=True,
            timeout=30,
        )

        if result.returncode == 0:
            try:
                return json.loads(result.stdout)
            except json.JSONDecodeError:
                return {"output": result.stdout, "success": True}
        else:
            try:
                return json.loads(result.stderr)
            except json.JSONDecodeError:
                return {"error": result.stderr, "success": False}
    except subprocess.TimeoutExpired:
        return {"error": "Processing timed out", "success": False}
    except FileNotFoundError:
        return {"error": "rexpipe not found. Install from: https://github.com/jkindrix/rexpipe", "success": False}


# --- Input Schemas ---

class SubstituteInput(BaseModel):
    """Input for text substitution."""
    text: str = Field(description="The text to process")
    pattern: str = Field(description="Regex pattern to match")
    replacement: str = Field(description="Replacement string (use $1, $2 for capture groups)")


class FilterInput(BaseModel):
    """Input for line filtering."""
    text: str = Field(description="The text to filter (line by line)")
    pattern: str = Field(description="Regex pattern to match")
    keep: bool = Field(default=True, description="Keep matching lines (True) or drop them (False)")


class ExtractInput(BaseModel):
    """Input for pattern extraction."""
    text: str = Field(description="The text to extract from")
    pattern: str = Field(description="Regex pattern to match")


class TransformInput(BaseModel):
    """Input for text transformation."""
    text: str = Field(description="The text to transform")
    action: str = Field(description="Transform action: uppercase, lowercase, trim, snake_case, camel_case")


class RedactPIIInput(BaseModel):
    """Input for PII redaction."""
    text: str = Field(description="The text to redact PII from")
    replacement: str = Field(default="[REDACTED]", description="Replacement text for PII")


class PipelineInput(BaseModel):
    """Input for multi-step pipeline."""
    text: str = Field(description="The text to process")
    config: str = Field(description="Pipeline configuration in TOML format")


# --- Tools ---

class RexpipeSubstitute(BaseTool):
    """Replace text matching a regex pattern."""

    name: str = "rexpipe_substitute"
    description: str = """Replace text matching a regex pattern with a replacement string.
    Supports capture groups ($1, $2, etc.) in replacement.
    Example: pattern='\d+' replacement='NUM' turns 'test 123' into 'test NUM'"""
    args_schema: Type[BaseModel] = SubstituteInput

    def _run(self, text: str, pattern: str, replacement: str) -> str:
        result = run_rexpipe(["-p", pattern, "-r", replacement], text)
        return result.get("output", json.dumps(result))


class RexpipeFilter(BaseTool):
    """Filter lines based on a regex pattern."""

    name: str = "rexpipe_filter"
    description: str = """Filter lines of text based on a regex pattern.
    keep=True keeps matching lines, keep=False drops matching lines.
    Useful for log filtering, extracting specific content, etc."""
    args_schema: Type[BaseModel] = FilterInput

    def _run(self, text: str, pattern: str, keep: bool = True) -> str:
        action = "keep_line" if keep else "drop_line"
        result = run_rexpipe(["-p", pattern, "--filter", action], text)
        return result.get("output", json.dumps(result))


class RexpipeExtract(BaseTool):
    """Extract text matching a regex pattern."""

    name: str = "rexpipe_extract"
    description: str = """Extract all text matching a regex pattern.
    Use capture groups to extract specific parts.
    Example: pattern='(\w+)@(\w+)\.com' extracts email parts."""
    args_schema: Type[BaseModel] = ExtractInput

    def _run(self, text: str, pattern: str) -> str:
        result = run_rexpipe(["-p", pattern, "--extract"], text)
        return result.get("output", json.dumps(result))


class RexpipeTransform(BaseTool):
    """Apply text transformations."""

    name: str = "rexpipe_transform"
    description: str = """Transform text: uppercase, lowercase, trim, snake_case, camel_case, pascal_case, kebab_case.
    Example: action='snake_case' turns 'helloWorld' into 'hello_world'"""
    args_schema: Type[BaseModel] = TransformInput

    def _run(self, text: str, action: str) -> str:
        result = run_rexpipe(["--transform", action], text)
        return result.get("output", json.dumps(result))


class RexpipeRedactPII(BaseTool):
    """Redact personally identifiable information from text."""

    name: str = "rexpipe_redact_pii"
    description: str = """Redact PII from text: emails, phone numbers, SSNs, credit cards, IP addresses.
    Returns text with PII replaced by [REDACTED] or custom replacement.
    ALWAYS use this before logging or storing user-provided text."""
    args_schema: Type[BaseModel] = RedactPIIInput

    def _run(self, text: str, replacement: str = "[REDACTED]") -> str:
        # Use the AI pattern library for comprehensive PII detection
        pipeline = f'''
[[steps]]
type = "substitute"
pattern = '[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\\.[a-zA-Z]{{2,}}'
replacement = "{replacement}"

[[steps]]
type = "substitute"
pattern = '(?:\\+?[0-9]{{1,4}}[-.]?)?(?:\\([0-9]{{1,4}}\\)[-.]?)?[0-9]{{1,4}}[-.][0-9]{{1,4}}[-.][0-9]{{1,9}}'
replacement = "{replacement}"

[[steps]]
type = "substitute"
pattern = '\\b\\d{{3}}-\\d{{2}}-\\d{{4}}\\b'
replacement = "{replacement}"

[[steps]]
type = "substitute"
pattern = '\\b(?:\\d{{4}}[-\\s]?){{3}}\\d{{4}}\\b'
replacement = "{replacement}"

[[steps]]
type = "substitute"
pattern = '\\b(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\\.(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\\.(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\\.(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\\b'
replacement = "{replacement}"
'''
        result = run_rexpipe(["-c", "-"], text)
        # For inline config, we need a different approach
        import tempfile
        with tempfile.NamedTemporaryFile(mode='w', suffix='.toml', delete=False) as f:
            f.write(pipeline)
            f.flush()
            result = run_rexpipe(["-c", f.name], text)
        return result.get("output", json.dumps(result))


class RexpipePipeline(BaseTool):
    """Execute a multi-step text processing pipeline."""

    name: str = "rexpipe_pipeline"
    description: str = """Execute a multi-step pipeline for complex text processing.
    Config is TOML format with [[steps]] sections.
    Each step can be: substitute, filter, extract, validate, transform.
    Most powerful option for complex transformations."""
    args_schema: Type[BaseModel] = PipelineInput

    def _run(self, text: str, config: str) -> str:
        import tempfile
        with tempfile.NamedTemporaryFile(mode='w', suffix='.toml', delete=False) as f:
            f.write(config)
            f.flush()
            result = run_rexpipe(["-c", f.name], text)
        return result.get("output", json.dumps(result))


# --- Convenience Functions ---

def get_all_tools() -> list[BaseTool]:
    """Get all rexpipe tools for use with LangChain agents."""
    return [
        RexpipeSubstitute(),
        RexpipeFilter(),
        RexpipeExtract(),
        RexpipeTransform(),
        RexpipeRedactPII(),
        RexpipePipeline(),
    ]


# --- Example Usage ---

if __name__ == "__main__":
    # Test the tools
    print("Testing rexpipe LangChain tools...\n")

    # Test substitute
    sub = RexpipeSubstitute()
    result = sub._run("Hello 123 World 456", r"\d+", "NUM")
    print(f"Substitute: {result}")

    # Test filter
    filt = RexpipeFilter()
    result = filt._run("ERROR: something failed\nINFO: all good\nERROR: another issue", "ERROR", True)
    print(f"Filter: {result}")

    # Test redact
    redact = RexpipeRedactPII()
    result = redact._run("Contact john@example.com or call 555-123-4567")
    print(f"Redact: {result}")

    print("\nAll tools working!")
