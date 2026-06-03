"""
CogEquippedTerminus — Terminus-2 agent with cog binary + inline skill docs.

Usage:
    harbor run \
        --agent-import-path benchmark.cog_terminus:CogEquippedTerminus \
        ...
"""

from pathlib import Path

from harbor.agents.terminus_2.terminus_2 import Terminus2
from harbor.environments.base import BaseEnvironment
from harbor.models.agent.context import AgentContext


# skills/ dir at repo root
_SKILLS_DIR = Path(__file__).resolve().parent.parent / "skills" / "cog"


class CogEquippedTerminus(Terminus2):
    """Terminus-2 that injects the cog binary and inlines all skill docs."""

    @staticmethod
    def name() -> str:
        return "cog-equipped-terminus-2"

    async def setup(self, environment: BaseEnvironment) -> None:
        await super().setup(environment)

        repo_root = Path(__file__).resolve().parent.parent
        local_cog_path = repo_root / "target" / "release" / "cog"

        if not local_cog_path.exists():
            raise FileNotFoundError(
                f"cog binary not found at {local_cog_path}. "
                "Run `cargo build --release` first."
            )

        await environment.upload_file(
            str(local_cog_path), "/usr/local/bin/cog"
        )
        await environment.exec("chmod +x /usr/local/bin/cog")

    async def run(
        self,
        instruction: str,
        environment: BaseEnvironment,
        context: AgentContext,
    ) -> None:
        skill_text = "\n\n".join(
            f.read_text(encoding="utf-8")
            for f in sorted(_SKILLS_DIR.glob("*.md"))
        )
        augmented = (
            instruction
            + "\n\n# cog — Cognitive Model for Coding Agents\n\n"
            + skill_text
        )
        await super().run(augmented, environment, context)
