"""
Configuração centralizada do Context Agent.
Todos os paths, constantes e limites usados pelos demais módulos.
"""

from pathlib import Path

import os

# ── Raízes Dinâmicas ────────────────────────────────────────────────
CONTEXT_AGENT_ROOT = Path(__file__).resolve().parent.parent
SKILLS_ROOT = CONTEXT_AGENT_ROOT.parent
WORKSPACE_ROOT = CONTEXT_AGENT_ROOT.parent.parent

# ── Dados do agente ─────────────────────────────────────────────────
DATA_DIR = CONTEXT_AGENT_ROOT / "data"
SESSIONS_DIR = DATA_DIR / "sessions"
ARCHIVE_DIR = DATA_DIR / "archive"
LOGS_DIR = DATA_DIR / "logs"
ACTIVE_CONTEXT_PATH = DATA_DIR / "ACTIVE_CONTEXT.md"
PROJECT_REGISTRY_PATH = DATA_DIR / "PROJECT_REGISTRY.md"
DB_PATH = DATA_DIR / "context.db"

# ── Detecção de Logs (Antigravity ou Claude Code) ────────────────────
USER_HOME = Path(os.path.expanduser("~"))
ANTIGRAVITY_BRAIN = USER_HOME / ".gemini" / "antigravity-ide" / "brain"

if ANTIGRAVITY_BRAIN.exists():
    # Mengambil brain folder terupdate yang valid (memiliki logs/transcript.jsonl)
    brain_dirs = sorted(
        [d for d in ANTIGRAVITY_BRAIN.iterdir() if d.is_dir() and not d.name.startswith(".") and (d / ".system_generated" / "logs" / "transcript.jsonl").exists()],
        key=lambda d: d.stat().st_mtime,
        reverse=True
    )
    if brain_dirs:
        CLAUDE_SESSION_DIR = brain_dirs[0] / ".system_generated" / "logs"
    else:
        CLAUDE_SESSION_DIR = CONTEXT_AGENT_ROOT / "data" / "sessions"
else:
    CLAUDE_PROJECTS_DIR = USER_HOME / ".claude" / "projects"
    if CLAUDE_PROJECTS_DIR.exists():
        project_dirs = sorted(
            [d for d in CLAUDE_PROJECTS_DIR.iterdir() if d.is_dir()],
            key=lambda d: d.stat().st_mtime,
            reverse=True
        )
        CLAUDE_SESSION_DIR = project_dirs[0] if project_dirs else (CONTEXT_AGENT_ROOT / "data" / "sessions")
    else:
        CLAUDE_SESSION_DIR = CONTEXT_AGENT_ROOT / "data" / "sessions"

MEMORY_DIR = CONTEXT_AGENT_ROOT / "data"
MEMORY_MD_PATH = MEMORY_DIR / "MEMORY.md"


# ── Limites ─────────────────────────────────────────────────────────
MAX_ACTIVE_CONTEXT_LINES = 150      # MEMORY.md é truncado em 200 linhas
MAX_RECENT_SESSIONS = 5             # Sessões recentes carregadas no briefing
ARCHIVE_AFTER_SESSIONS = 20         # Arquivar sessões mais antigas que N
MAX_DECISIONS_AGE_DAYS = 30         # Decisões mais velhas são podadas
MAX_SEARCH_RESULTS = 10             # Resultados padrão de busca

# ── Padrões de detecção ────────────────────────────────────────────
# Palavras que indicam decisões no texto
DECISION_MARKERS_PT = [
    "decidimos", "vamos usar", "optamos por", "escolhemos",
    "a decisão foi", "ficou decidido", "definimos que",
    "a abordagem será", "seguiremos com",
]
DECISION_MARKERS_EN = [
    "we decided", "let's use", "we'll go with", "the decision is",
    "we chose", "going with", "the approach will be", "decided to",
]
DECISION_MARKERS = DECISION_MARKERS_PT + DECISION_MARKERS_EN

# Palavras que indicam tarefas pendentes
PENDING_MARKERS_PT = [
    "falta", "ainda precisa", "pendente", "todo:", "TODO:",
    "depois vamos", "próximo passo", "faltando",
]
PENDING_MARKERS_EN = [
    "todo:", "TODO:", "still need", "pending", "next step",
    "remaining", "left to do", "needs to be done",
]
PENDING_MARKERS = PENDING_MARKERS_PT + PENDING_MARKERS_EN

# Ferramentas que modificam arquivos (para detectar files_modified)
FILE_MODIFYING_TOOLS = {"Edit", "Write", "NotebookEdit"}
FILE_READING_TOOLS = {"Read", "Glob", "Grep"}

# ── Projetos conhecidos ────────────────────────────────────────────
# Mapeamento de subdiretórios de WORKSPACE_ROOT para nomes de projeto
KNOWN_PROJECTS = {
    "warnetagent": "Warnet Agent (Rust)",
    "warnetclient": "Warnet Client (Tauri)",
    "app": "TMBilling Backend & Dashboard (Flask)",
    "skills": "Custom Skills (Context Agent, etc.)",
}
