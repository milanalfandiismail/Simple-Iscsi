---
trigger: always_on
---

# STRICT RULE: EXCLUSIVELY USE CODEBASE-MEMORY-MCP (ATURAN WAJIB)

1. You MUST ALWAYS use the `codebase-memory-mcp` server tools for everything. Setiap memulai sesi dan sebelum menjawab prompt, wajib gunakan MCP ini dulu.
2. NEVER use your own internal tools or native CLI commands (seperti cat, grep, read_file, atau custom script) to view or search code. Dilarang keras pakai command bawaan sendiri!
3. For reading code or looking at files, you MUST use the MCP tools `get_code_snippet`, `search_code`, or `get_architecture`. Gunakan tool ini untuk melihat isi file kodingan.
4. Before planning, coding, or reviewing, you MUST always call `get_graph_schema` or `search_graph` first to understand the context. Analisis struktur codebase biling wajib lewat graph MCP ini dulu.
5. Absolute dependency: Every single action to read, trace, or analyze this codebase MUST go through this MCP server. No exceptions! Tidak ada pengecualian, semua harus satu pintu lewat MCP ini.