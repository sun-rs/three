# three (Codex skill pack)

This directory provides a Codex-native equivalent of the Claude plugin by using
Codex skills.

Codex currently does not use Claude's marketplace plugin manifest format; use
skills instead.

## Install

1) Register MCP server in Codex:

```bash
codex mcp add three -- "$(pwd)/mcp-server-three/target/release/mcp-server-three"
```

2) Install skills (recommended):

```bash
mkdir -p ~/.codex/skills
for d in "$(pwd)"/plugins/codex/three/skills/*; do
  name="$(basename "$d")"
  ln -sfn "$d" "$HOME/.codex/skills/$name"
done
```

3) Restart Codex.

## Use

- `$three-conductor` for orchestration and delegation
- `$three-roundtable` for 1-3 discussion rounds with consensus synthesis
- `$three-oracle|three-builder|three-researcher|three-reviewer|three-critic|three-sprinter` for direct specialist calls
- `$three-info` for role/model diagnostics
- `$three-routing` for routing/delegation policy

All skills call MCP with `client: "codex"`, so the server prefers
`config-codex.json` when present.

Role availability is runtime-dynamic: always trust `mcp__three__info` (`enabled=true`) as the callable set.
