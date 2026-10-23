# Agents

Agents are principals of their own, sponsored by a person or team, with
budgets and scopes.

    rill agent register reviewer --sponsor team:infra --repos app --purpose "first review"
    rill agent list
    rill admin kill --agent reviewer  # stop it and end its sessions

Agents work through the MCP server (`rill-mcp`) or the SDKs. Every change
records its provenance (tool, model, session), which `rill show` prints.
