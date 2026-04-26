export type Tool = { name: string; summary: string; detail?: string };

export const tools: Tool[] = [
  {
    name: 'list_sections',
    summary: 'Browse indexed std and popular-crate documentation.',
    detail: 'Returns paths into the bundled corpus snapshot. Filterable by prefix.'
  },
  {
    name: 'get_documentation',
    summary: 'Markdown for a single section path returned by list_sections.',
    detail: 'Read-only SQLite + zstd blobs, served from the binary.'
  },
  {
    name: 'clippy_check',
    summary: 'Lint a Rust snippet and return JSON diagnostics.',
    detail: 'Runs cargo clippy in a tempdir under bounded time and output limits.'
  },
  {
    name: 'clippy_fix',
    summary: 'Auto-apply clippy suggestions to a snippet.'
  },
  {
    name: 'rustfmt',
    summary: 'Format a snippet via rustfmt --emit stdout.'
  },
  {
    name: 'playground_link',
    summary: 'Build a shareable play.rust-lang.org permalink.'
  },
  {
    name: 'playground_run',
    summary: 'Execute a snippet on the official Rust Playground sandbox.'
  },
  {
    name: 'crate_search',
    summary: 'Search crates.io and return top matches.'
  },
  {
    name: 'crate_info',
    summary: 'Versions, features, dependencies, repository, docs.rs URL for a crate.'
  },
  {
    name: 'rustc_explain',
    summary: 'Reference text for a compiler error code (E0XXX).'
  },
  {
    name: 'run_locally',
    summary: 'Emit a plan of shell commands the calling agent should run.',
    detail: 'The server itself never executes user code; the trust boundary stays at the agent host.'
  }
];
