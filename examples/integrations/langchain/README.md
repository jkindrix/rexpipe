# rexpipe LangChain Integration

Use rexpipe as tools in LangChain agents for AI-powered text processing.

## Installation

```bash
# Install rexpipe
cargo install rexpipe

# Install Python dependencies
pip install langchain langchain-openai pydantic
```

## Quick Start

```python
from langchain_openai import ChatOpenAI
from langchain.agents import create_tool_calling_agent, AgentExecutor
from langchain_core.prompts import ChatPromptTemplate

from rexpipe_tools import get_all_tools

# Create LLM and tools
llm = ChatOpenAI(model="gpt-4")
tools = get_all_tools()

# Create agent
prompt = ChatPromptTemplate.from_messages([
    ("system", "You are a helpful assistant that processes text using rexpipe tools."),
    ("human", "{input}"),
    ("placeholder", "{agent_scratchpad}"),
])

agent = create_tool_calling_agent(llm, tools, prompt)
executor = AgentExecutor(agent=agent, tools=tools, verbose=True)

# Run
result = executor.invoke({
    "input": "Remove all email addresses from: Contact support@company.com or sales@company.com"
})
print(result["output"])
```

## Available Tools

| Tool | Description |
|------|-------------|
| `rexpipe_substitute` | Replace text matching regex patterns |
| `rexpipe_filter` | Filter lines by pattern |
| `rexpipe_extract` | Extract matching text |
| `rexpipe_transform` | Transform text (uppercase, snake_case, etc.) |
| `rexpipe_redact_pii` | Redact emails, phones, SSNs, etc. |
| `rexpipe_pipeline` | Multi-step TOML pipelines |

## Examples

### Data Cleaning Agent

```python
from rexpipe_tools import RexpipeSubstitute, RexpipeRedactPII, RexpipeTransform

tools = [RexpipeSubstitute(), RexpipeRedactPII(), RexpipeTransform()]

# Agent will automatically choose the right tool
executor.invoke({
    "input": "Clean this data: normalize whitespace, remove PII, convert to lowercase"
})
```

### Log Analysis Agent

```python
from rexpipe_tools import RexpipeFilter, RexpipeExtract

tools = [RexpipeFilter(), RexpipeExtract()]

executor.invoke({
    "input": "Filter this log to only show errors and extract the timestamps"
})
```

### Code Processing Agent

```python
from rexpipe_tools import RexpipePipeline

pipeline_tool = RexpipePipeline()

# Complex multi-step transformation
result = pipeline_tool._run(
    text="function helloWorld() { console.log('test'); }",
    config='''
[[steps]]
type = "substitute"
pattern = "console\\.log"
replacement = "logger.info"

[[steps]]
type = "transform"
action = "snake_case"
'''
)
```

## LangGraph Integration

```python
from langgraph.graph import StateGraph
from rexpipe_tools import get_all_tools

# Define state
class State(TypedDict):
    text: str
    processed: str

# Create nodes that use rexpipe tools
def clean_node(state: State) -> State:
    redact = RexpipeRedactPII()
    state["processed"] = redact._run(state["text"])
    return state

# Build graph
graph = StateGraph(State)
graph.add_node("clean", clean_node)
```

## Best Practices

### 1. Use PII Redaction First

Always redact PII before logging or storing user input:

```python
redact = RexpipeRedactPII()
safe_text = redact._run(user_input)
logger.info(f"Processing: {safe_text}")
```

### 2. Validate Before Transform

Use the validation step to ensure data quality:

```python
# Check all lines are valid emails before processing
result = run_rexpipe(["-p", r"^[\w.]+@[\w.]+$", "--validate"], email_list)
if result.get("valid"):
    # Safe to process
    pass
```

### 3. Chain Tools for Complex Operations

```python
# First extract, then transform, then validate
extracted = RexpipeExtract()._run(text, r"\d{3}-\d{4}")
transformed = RexpipeTransform()._run(extracted, "trim")
```

## Error Handling

Tools return JSON with error information:

```python
result = run_rexpipe(["-p", "[invalid("], "test")
if "error" in result:
    print(f"Error: {result['error']}")
    print(f"Suggestion: {result.get('suggestion', 'Check pattern syntax')}")
```

## Performance Tips

1. **Use pipelines for multiple steps** - Single rexpipe call is faster than multiple
2. **Prefer pattern libraries** - Pre-tested, optimized patterns
3. **Use `--json`** - Structured output avoids parsing overhead
4. **Batch processing** - Process multiple files in one call with `-r`

## Related

- [MCP Integration](../mcp/) - For Claude and ChatGPT native integration
- [Claude Tools](../claude/) - For Anthropic Claude API
- [AI Cookbook](../../AI_COOKBOOK.md) - Practical recipes
