import json
import os
import time
from pathlib import Path
import tkinter as tk
from tkinter import ttk

MODULE_ID = "template_python_module"
MODULE_TITLE = "Template Python Module"
FIELD_KEYS = [
    "module_title",
    "current_focus",
    "recent_changes",
    "next_action",
    "artifacts",
]


class TemplatePythonApp:
    def __init__(self) -> None:
        self.module_root = Path(__file__).resolve().parent.parent
        self.state_path = self.module_root / "state.json"
        bridge_path = os.environ.get("CHATTYEDU_BRIDGE_STATUS", "").strip()
        self.bridge_status_path = Path(bridge_path) if bridge_path else None
        shared_state_path = os.environ.get("CHATTYEDU_BRIDGE_SHARED_STATE", "").strip()
        self.bridge_shared_state_path = Path(shared_state_path) if shared_state_path else None
        incoming_state_path = os.environ.get("CHATTYEDU_BRIDGE_INCOMING_SHARED_STATE", "").strip()
        self.bridge_incoming_shared_state_path = (
            Path(incoming_state_path) if incoming_state_path else None
        )
        self.last_bridge_fingerprint = ""
        self.last_shared_state_fingerprint = ""
        self.last_incoming_shared_state_fingerprint = ""

        self.root = tk.Tk()
        self.root.title(MODULE_TITLE)
        self.root.geometry("1040x720")
        self.root.minsize(780, 540)

        self.status_var = tk.StringVar(value="Ready.")
        self.completion_var = tk.StringVar(value="0%")
        self.artifact_count_var = tk.StringVar(value="0")
        self.bridge_mode_var = tk.StringVar(
            value="hosted" if self.bridge_status_path else "standalone"
        )

        self.module_title_var = tk.StringVar()
        self.current_focus_var = tk.StringVar()

        self._build_ui()
        self._load_state()
        self.refresh_ui()
        self.root.after(1500, self._poll_incoming_shared_state)

    def _build_ui(self) -> None:
        self.root.columnconfigure(0, weight=0, minsize=250)
        self.root.columnconfigure(1, weight=1)
        self.root.rowconfigure(1, weight=1)

        toolbar = ttk.Frame(self.root, padding=10)
        toolbar.grid(row=0, column=0, columnspan=2, sticky="ew")
        toolbar.columnconfigure(4, weight=1)

        ttk.Button(toolbar, text="Save", command=self.save_state).grid(
            row=0, column=0, padx=(0, 8)
        )
        ttk.Button(toolbar, text="Reset", command=self.reset_state).grid(
            row=0, column=1, padx=(0, 8)
        )
        ttk.Button(toolbar, text="Refresh preview", command=self.refresh_ui).grid(
            row=0, column=2, padx=(0, 8)
        )
        ttk.Label(toolbar, textvariable=self.status_var).grid(row=0, column=4, sticky="e")

        sidebar = ttk.Frame(self.root, padding=14)
        sidebar.grid(row=1, column=0, sticky="nsew")
        sidebar.columnconfigure(0, weight=1)

        ttk.Label(sidebar, text="Template Python Module", font=("Segoe UI", 15, "bold")).grid(
            row=0, column=0, sticky="w"
        )
        ttk.Label(
            sidebar,
            text="This starter keeps app state in state.json and only reports through the optional Chatty-EDU bridge when hosted.",
            wraplength=210,
            justify="left",
        ).grid(row=1, column=0, sticky="w", pady=(8, 14))
        ttk.Label(sidebar, text="Fields filled").grid(row=2, column=0, sticky="w")
        ttk.Label(sidebar, textvariable=self.completion_var, font=("Segoe UI", 14, "bold")).grid(
            row=3, column=0, sticky="w", pady=(0, 8)
        )
        ttk.Label(sidebar, text="Artifacts listed").grid(row=4, column=0, sticky="w")
        ttk.Label(sidebar, textvariable=self.artifact_count_var, font=("Segoe UI", 14, "bold")).grid(
            row=5, column=0, sticky="w", pady=(0, 8)
        )
        ttk.Label(sidebar, text="Bridge mode").grid(row=6, column=0, sticky="w")
        ttk.Label(sidebar, textvariable=self.bridge_mode_var, font=("Segoe UI", 12, "bold")).grid(
            row=7, column=0, sticky="w"
        )

        content = ttk.Frame(self.root, padding=(8, 10, 12, 12))
        content.grid(row=1, column=1, sticky="nsew")
        content.columnconfigure(0, weight=1)
        content.columnconfigure(1, weight=1)
        content.rowconfigure(0, weight=1)

        desk = ttk.LabelFrame(content, text="Module desk", padding=12)
        desk.grid(row=0, column=0, sticky="nsew", padx=(0, 8))
        desk.columnconfigure(0, weight=1)

        preview = ttk.LabelFrame(content, text="Handoff preview", padding=12)
        preview.grid(row=0, column=1, sticky="nsew")
        preview.columnconfigure(0, weight=1)
        preview.rowconfigure(3, weight=1)

        ttk.Label(desk, text="Module title").grid(row=0, column=0, sticky="w")
        title_entry = ttk.Entry(desk, textvariable=self.module_title_var)
        title_entry.grid(row=1, column=0, sticky="ew", pady=(0, 10))
        title_entry.bind("<KeyRelease>", self._on_change)

        ttk.Label(desk, text="Current focus").grid(row=2, column=0, sticky="w")
        focus_entry = ttk.Entry(desk, textvariable=self.current_focus_var)
        focus_entry.grid(row=3, column=0, sticky="ew", pady=(0, 10))
        focus_entry.bind("<KeyRelease>", self._on_change)

        self.recent_changes_text = self._make_text_area(desk, "Recent changes", 4)
        self.next_action_text = self._make_text_area(desk, "Next action", 6)
        self.artifacts_text = self._make_text_area(desk, "Artifacts", 8)

        ttk.Label(
            preview,
            text="This is the kind of summary/snapshot Chatty-EDU can read when the module is hosted.",
            wraplength=360,
            justify="left",
        ).grid(row=0, column=0, sticky="w", pady=(0, 10))
        ttk.Label(preview, text="Summary").grid(row=1, column=0, sticky="w")
        self.summary_preview = self._make_preview(preview, row=2, height=7)
        ttk.Label(preview, text="Snapshot").grid(row=3, column=0, sticky="w", pady=(10, 0))
        self.snapshot_preview = self._make_preview(preview, row=4, height=18, stretchy=True)

    def _make_text_area(self, parent: ttk.Frame, label: str, row: int) -> tk.Text:
        ttk.Label(parent, text=label).grid(row=row, column=0, sticky="w")
        text = tk.Text(parent, height=6, wrap="word")
        text.grid(row=row + 1, column=0, sticky="nsew", pady=(0, 10))
        text.bind("<KeyRelease>", self._on_change)
        parent.rowconfigure(row + 1, weight=1)
        return text

    def _make_preview(
        self, parent: ttk.Frame, row: int, height: int, stretchy: bool = False
    ) -> tk.Text:
        text = tk.Text(parent, height=height, wrap="word")
        text.grid(row=row, column=0, sticky="nsew", pady=(4, 0))
        text.configure(state="disabled")
        if stretchy:
            parent.rowconfigure(row, weight=1)
        return text

    def _on_change(self, _event=None) -> None:
        self.refresh_ui()

    def _load_state(self) -> None:
        if not self.state_path.is_file():
            return
        try:
            state = json.loads(self.state_path.read_text(encoding="utf-8"))
        except Exception:
            return

        self.module_title_var.set(state.get("module_title", ""))
        self.current_focus_var.set(state.get("current_focus", ""))
        self._set_text(self.recent_changes_text, state.get("recent_changes", ""))
        self._set_text(self.next_action_text, state.get("next_action", ""))
        self._set_text(self.artifacts_text, state.get("artifacts", ""))

    def _collect_state(self) -> dict:
        return {
            "module_title": self.module_title_var.get(),
            "current_focus": self.current_focus_var.get(),
            "recent_changes": self._get_text(self.recent_changes_text),
            "next_action": self._get_text(self.next_action_text),
            "artifacts": self._get_text(self.artifacts_text),
        }

    def save_state(self) -> None:
        try:
            self.state_path.write_text(
                json.dumps(self._collect_state(), indent=2),
                encoding="utf-8",
            )
            self.status_var.set(f"Saved state to {self.state_path}")
        except Exception as exc:
            self.status_var.set(f"Save failed: {exc}")
        self.refresh_ui()

    def reset_state(self) -> None:
        self.module_title_var.set("")
        self.current_focus_var.set("")
        self._set_text(self.recent_changes_text, "")
        self._set_text(self.next_action_text, "")
        self._set_text(self.artifacts_text, "")
        if self.bridge_status_path and self.bridge_status_path.is_file():
            try:
                self.bridge_status_path.unlink()
            except Exception:
                pass
        if self.bridge_shared_state_path and self.bridge_shared_state_path.is_file():
            try:
                self.bridge_shared_state_path.unlink()
            except Exception:
                pass
        self.status_var.set("State reset.")
        self.save_state()

    def refresh_ui(self) -> None:
        state = self._collect_state()
        completion = round((self._field_completion(state) / len(FIELD_KEYS)) * 100)
        artifact_count = len(self._meaningful_lines(state["artifacts"]))
        summary = self._bridge_summary(state, completion, artifact_count)
        snapshot = self._bridge_snapshot(state)

        self.completion_var.set(f"{completion}%")
        self.artifact_count_var.set(str(artifact_count))
        self.bridge_mode_var.set("hosted" if self.bridge_status_path else "standalone")
        self._set_preview(self.summary_preview, summary)
        self._set_preview(self.snapshot_preview, snapshot)
        self._sync_bridge(summary, snapshot, completion, artifact_count)
        self._sync_shared_state(state, summary, completion, artifact_count)

    def _field_completion(self, state: dict) -> int:
        return sum(1 for key in FIELD_KEYS if state.get(key, "").strip())

    def _meaningful_lines(self, value: str) -> list[str]:
        return [line.strip() for line in value.splitlines() if line.strip()]

    def _bridge_summary(self, state: dict, completion: int, artifact_count: int) -> str:
        title = state["module_title"].strip() or MODULE_TITLE
        focus = state["current_focus"].strip() or "no current focus set"
        next_action = state["next_action"].strip() or "define the next action"
        return (
            f"{title} is {completion}% filled out. "
            f"Current focus: {focus}. "
            f"Artifacts listed: {artifact_count}. "
            f"Next handoff focus: {next_action}."
        )

    def _bridge_snapshot(self, state: dict) -> str:
        artifacts = self._meaningful_lines(state["artifacts"])
        return "\n".join(
            [
                "# Template Python Module Snapshot",
                "",
                f"- Module title: {state['module_title'].strip() or 'not set'}",
                f"- Current focus: {state['current_focus'].strip() or 'not set'}",
                "",
                "## Recent changes",
                state["recent_changes"].strip() or "(empty)",
                "",
                "## Next action",
                state["next_action"].strip() or "(empty)",
                "",
                "## Artifacts",
                "\n".join(artifacts) if artifacts else "(none)",
            ]
        )

    def _sync_bridge(
        self, summary: str, snapshot: str, completion: int, artifact_count: int
    ) -> None:
        if not self.bridge_status_path:
            return

        payload = {
            "module_id": MODULE_ID,
            "event_type": "suspend_rundown",
            "summary": summary,
            "snapshot": snapshot,
            "tags": ["template", "python", "native_window", "starter"],
            "payload": {
                "completion": completion,
                "artifact_count": artifact_count,
                "status": self.status_var.get(),
            },
            "updated_at_unix_ms": int(time.time() * 1000),
        }

        fingerprint = json.dumps(payload, sort_keys=True)
        if fingerprint == self.last_bridge_fingerprint:
            return

        try:
            self.bridge_status_path.parent.mkdir(parents=True, exist_ok=True)
            self.bridge_status_path.write_text(
                json.dumps(payload, indent=2), encoding="utf-8"
            )
            self.last_bridge_fingerprint = fingerprint
        except Exception:
            pass

    def _sync_shared_state(
        self, state: dict, summary: str, completion: int, artifact_count: int
    ) -> None:
        if not self.bridge_shared_state_path:
            return

        payload = {
            "module_id": MODULE_ID,
            "summary": summary,
            "payload": {
                "fields": state,
                "metrics": {
                    "completion": completion,
                    "artifact_count": artifact_count,
                },
            },
            "updated_at_unix_ms": int(time.time() * 1000),
        }

        fingerprint = json.dumps(payload, sort_keys=True)
        if fingerprint == self.last_shared_state_fingerprint:
            return

        try:
            self.bridge_shared_state_path.parent.mkdir(parents=True, exist_ok=True)
            self.bridge_shared_state_path.write_text(
                json.dumps(payload, indent=2), encoding="utf-8"
            )
            self.last_shared_state_fingerprint = fingerprint
        except Exception:
            pass

    def _poll_incoming_shared_state(self) -> None:
        try:
            if (
                self.bridge_incoming_shared_state_path
                and self.bridge_incoming_shared_state_path.is_file()
            ):
                raw = self.bridge_incoming_shared_state_path.read_text(encoding="utf-8")
                if raw.strip():
                    fingerprint = raw.strip()
                    if fingerprint != self.last_incoming_shared_state_fingerprint:
                        incoming = json.loads(raw)
                        if (
                            incoming.get("module_id", "").strip() == MODULE_ID
                            and isinstance(incoming.get("payload"), dict)
                        ):
                            fields = incoming["payload"].get("fields", {})
                            if isinstance(fields, dict):
                                self.module_title_var.set(fields.get("module_title", ""))
                                self.current_focus_var.set(fields.get("current_focus", ""))
                                self._set_text(
                                    self.recent_changes_text,
                                    fields.get("recent_changes", ""),
                                )
                                self._set_text(
                                    self.next_action_text,
                                    fields.get("next_action", ""),
                                )
                                self._set_text(
                                    self.artifacts_text,
                                    fields.get("artifacts", ""),
                                )
                                self.last_incoming_shared_state_fingerprint = fingerprint
                                try:
                                    self.state_path.write_text(
                                        json.dumps(self._collect_state(), indent=2),
                                        encoding="utf-8",
                                    )
                                except Exception:
                                    pass
                                sender = incoming.get("from_device_name", "").strip() or "peer"
                                self.status_var.set(
                                    f"Applied shared module state from {sender}."
                                )
                                self.refresh_ui()
        except Exception:
            pass
        finally:
            self.root.after(1500, self._poll_incoming_shared_state)

    def _get_text(self, widget: tk.Text) -> str:
        return widget.get("1.0", "end").strip()

    def _set_text(self, widget: tk.Text, value: str) -> None:
        widget.delete("1.0", "end")
        widget.insert("1.0", value)

    def _set_preview(self, widget: tk.Text, value: str) -> None:
        widget.configure(state="normal")
        widget.delete("1.0", "end")
        widget.insert("1.0", value)
        widget.configure(state="disabled")

    def run(self) -> None:
        self.root.mainloop()


if __name__ == "__main__":
    TemplatePythonApp().run()
