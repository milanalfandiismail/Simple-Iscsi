---
trigger: always_on
---

# STRICT RULE: EXCLUSIVELY USE CODEBASE-MEMORY-MCP

## Wajib diikuti tanpa pengecualian

1. **Selalu mulai sesi** dengan `get_graph_schema` atau `list_projects` untuk orientasi.
2. **Dilarang keras** pakai `cat`, `grep`, `read_file`, `ls`, `find`, atau command native apapun untuk membaca/mencari kode.
3. **Sebelum planning, coding, atau review** → wajib panggil `get_architecture` atau `get_graph_schema` dulu.

---

## Urutan wajib saat membaca kode

```
1. search_graph(name_pattern=".*NamaYangDicari.*") → dapat qualified name
2. get_code_snippet(qualified_name="project.path.NamaFungsi") → baca isinya
```

JANGAN langsung `get_code_snippet` tanpa tahu qualified name-nya. Cari dulu pakai `search_graph`.

---

## Format argumen yang benar (copy-paste ready)

### index_repository
```json
{"repo_path": "/absolute/path/to/project"}
```
> ⚠️ Wajib absolute path. Relative path akan gagal.

### search_graph
```json
{"name_pattern": ".*Handler.*", "label": "Function", "project": "nama-project"}
```
Label valid: `Function`, `Class`, `Method`, `File`, `Route`, `Module`, `Interface`

### get_code_snippet
```json
{"qualified_name": "myproject.src.handlers.ProcessOrder", "project": "nama-project"}
```
> Qualified name = `<project>.<folder>.<subfolder>.<nama_symbol>`. Dapatkan dari hasil `search_graph` dulu.

### trace_call_path
```json
{"function_name": "ProcessOrder", "direction": "both", "depth": 3}
```
Direction valid: `"inbound"`, `"outbound"`, `"both"`

### search_code (grep-like)
```json
{"query": "teks yang dicari", "project": "nama-project"}
```

### get_architecture
```json
{"project": "nama-project"}
```

### query_graph (Cypher)
```json
{"query": "MATCH (f:Function)-[:CALLS]->(g) WHERE f.name = 'main' RETURN g.name LIMIT 10"}
```

### detect_changes
```json
{"project": "nama-project"}
```

---

## Kalau tool error / tidak ketemu hasil

- **`get_code_snippet` gagal?** → Jalankan `search_graph` dulu untuk cari qualified name yang tepat.
- **`trace_call_path` 0 hasil?** → Pakai `search_graph(name_pattern=".*PartialName.*")` untuk verifikasi nama.
- **Tidak tahu nama project?** → Jalankan `list_projects` terlebih dahulu.
- **Masih gagal setelah 2 percobaan dengan MCP?** → Laporkan error-nya ke user. **JANGAN fallback ke native CLI.**

---

## Yang boleh dan tidak boleh

| ✅ Boleh | ❌ Dilarang |
|---|---|
| `search_graph` untuk cari simbol | `grep` / `rg` / `find` |
| `get_code_snippet` untuk baca kode | `cat` / `read_file` / `open` |
| `search_code` untuk cari teks | Native file search apapun |
| `get_architecture` untuk overview | Asumsi struktur tanpa query |
| `trace_call_path` untuk lacak alur | Manual trace lewat baca file |
| `query_graph` untuk Cypher query | Bash/script untuk analisis kode |
| `detect_changes` untuk lihat diff | `git diff` langsung |

---

## Referensi semua tool (14 tools)

| Tool | Kegunaan |
|---|---|
| `index_repository` | Index repo ke graph. Wajib pakai absolute path. |
| `list_projects` | Lihat semua project yang sudah diindex. |
| `delete_project` | Hapus project dari graph. |
| `index_status` | Cek status indexing project. |
| `get_graph_schema` | Lihat schema graph (node/edge counts). **Jalankan ini pertama.** |
| `get_architecture` | Overview codebase: bahasa, routes, hotspots, cluster. |
| `search_graph` | Cari simbol by nama, label, file pattern. |
| `get_code_snippet` | Baca source code fungsi by qualified name. |
| `search_code` | Grep-like search dalam file yang sudah diindex. |
| `trace_call_path` | BFS traversal — siapa yang memanggil / dipanggil fungsi ini. |
| `detect_changes` | Map git diff ke simbol yang terdampak + risk classification. |
| `query_graph` | Jalankan Cypher query (read-only). |
| `manage_adr` | CRUD Architecture Decision Records. |
| `ingest_traces` | Ingest runtime traces untuk validasi HTTP_CALLS edges. |