# @wae/open-smt-skills

Agent instructions for OpenSMT: the command surface, the preflight steps and the
evidence rules an assistant needs to prepare an SMT build without inventing a
capability.

## Install

```bash
pnpm add @wae/open-smt-skills
```

Point your agent at the skill file:

```text
node_modules/@wae/open-smt-skills/skills/open-smt/SKILL.md
```

## Example prompt

> Import `placements.csv` into a new document, add a 4-field feeder set for the
> 0603 and QFN packages, run validation, print the placement program, and export
> the placement drawing and the first-article sheet.

The skill tells the agent to run `open-smt capabilities` first, to report the
commands it actually ran, and to state which feeders are short instead of
assuming the setup is complete.

## Links

- [OpenSMT skill](./skills/open-smt/SKILL.md)
- [OpenSMT README](../../../README.md)
- [CLI reference](../../../documentation/cli.md)
- [Capabilities](../../../documentation/capabilities.md)
