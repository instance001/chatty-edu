import json
import os
import time
from datetime import datetime
from pathlib import Path
import tkinter as tk
from tkinter import ttk

MODULE_ID = "demo_teacher_notebook"
MODULE_TITLE = "Teacher Notebook (Demo)"
FIELD_KEYS = [
    "class_name",
    "learner_group",
    "bright_spots",
    "watch_items",
    "support_moves",
    "next_follow_up",
]


class TeacherNotebookApp:
    def __init__(self) -> None:
        self.module_root = Path(__file__).resolve().parent.parent
        self.state_path = self.module_root / "state.json"
        self.log_path = self.module_root / "logs" / "session_log.md"
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
        self.root.geometry("1080x740")
        self.root.minsize(800, 560)

        self.status_var = tk.StringVar(value="Ready.")
        self.completion_var = tk.StringVar(value="0%")
        self.watch_count_var = tk.StringVar(value="0")
        self.bridge_mode_var = tk.StringVar(
            value="hosted" if self.bridge_status_path else "standalone"
        )

        self.class_name_var = tk.StringVar()
        self.learner_group_var = tk.StringVar()

        self._build_ui()
        self._load_state()
        self.refresh_ui()
        self.root.after(1500, self._poll_incoming_shared_state)

    def _build_ui(self) -> None:
        self.root.columnconfigure(0, weight=0, minsize=260)
        self.root.columnconfigure(1, weight=1)
        self.root.rowconfigure(1, weight=1)

        toolbar = ttk.Frame(self.root, padding=10)
        toolbar.grid(row=0, column=0, columnspan=2, sticky="ew")
        toolbar.columnconfigure(4, weight=1)

        ttk.Button(toolbar, text="Save checkpoint", command=self.save_state).grid(
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

        ttk.Label(sidebar, text=MODULE_TITLE, font=("Segoe UI", 15, "bold")).grid(
            row=0, column=0, sticky="w"
        )
        ttk.Label(
            sidebar,
            text=(
                "Teacher-facing native demo. It keeps state in its own files, "
                "and only shares a bridge summary when hosted by Chatty-EDU."
            ),
            wraplength=220,
            justify="left",
        ).grid(row=1, column=0, sticky="w", pady=(8, 14))
        ttk.Label(sidebar, text="Fields filled").grid(row=2, column=0, sticky="w")
        ttk.Label(sidebar, textvariable=self.completion_var, font=("Segoe UI", 14, "bold")).grid(
            row=3, column=0, sticky="w", pady=(0, 8)
        )
        ttk.Label(sidebar, text="Watch items").grid(row=4, column=0, sticky="w")
        ttk.Label(sidebar, textvariable=self.watch_count_var, font=("Segoe UI", 14, "bold")).grid(
            row=5, column=0, sticky="w", pady=(0, 8)
        )
        ttk.Label(sidebar, text="Bridge mode").grid(row=6, column=0, sticky="w")
        ttk.Label(sidebar, textvariable=self.bridge_mode_var, font=("Segoe UI", 12, "bold")).grid(
            row=7, column=0, sticky="w", pady=(0, 8)
        )
        ttk.Label(sidebar, text=f"Log file: {self.log_path}", wraplength=220, justify="left").grid(
            row=8, column=0, sticky="w"
        )

        content = ttk.Frame(self.root, padding=(8, 10, 12, 12))
        content.grid(row=1, column=1, sticky="nsew")
        content.columnconfigure(0, weight=1)
        content.columnconfigure(1, weight=1)
        content.rowconfigure(0, weight=1)

        desk = ttk.LabelFrame(content, text="Observation desk", padding=12)
        desk.grid(row=0, column=0, sticky="nsew", padx=(0, 8))
        desk.columnconfigure(0, weight=1)

        preview = ttk.LabelFrame(content, text="Bridge handoff", padding=12)
        preview.grid(row=0, column=1, sticky="nsew")
        preview.columnconfigure(0, weight=1)
        preview.rowconfigure(3, weight=1)

        ttk.Label(desk, text="Class or subject").grid(row=0, column=0, sticky="w")
        class_entry = ttk.Entry(desk, textvariable=self.class_name_var)
        class_entry.grid(row=1, column=0, sticky="ew", pady=(0, 10))
        class_entry.bind("<KeyRelease>", self._on_change)

        ttk.Label(desk, text="Learner group").grid(row=2, column=0, sticky="w")
        group_entry = ttk.Entry(desk, textvariable=self.learner_group_var)
        group_entry.grid(row=3, column=0, sticky="ew", pady=(0, 10))
        group_entry.bind("<KeyRelease>", self._on_change)

        self.bright_spots_text = self._make_text_area(desk, "Bright spots", 4)
        self.watch_items_text = self._make_text_area(desk, "Watch items", 6)
        self.support_moves_text = self._make_text_area(desk, "Support moves", 8)
        self.next_follow_up_text = self._make_text_area(desk, "Next follow-up", 10)

        ttk.Label(
            preview,
            text=(
                "When hosted, Chatty-EDU can read this module's summary, snapshot, "
                "and recent log excerpt without taking over the module runtime."
            ),
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

        self.class_name_var.set(state.get("class_name", ""))
        self.learner_group_var.set(state.get("learner_group", ""))
        self._set_text(self.bright_spots_text, state.get("bright_spots", ""))
        self._set_text(self.watch_items_text, state.get("watch_items", ""))
        self._set_text(self.support_moves_text, state.get("support_moves", ""))
        self._set_text(self.next_follow_up_text, state.get("next_follow_up", ""))

    def _collect_state(self) -> dict:
        return {
            "class_name": self.class_name_var.get(),
            "learner_group": self.learner_group_var.get(),
            "bright_spots": self._get_text(self.bright_spots_text),
            "watch_items": self._get_text(self.watch_items_text),
            "support_moves": self._get_text(self.support_moves_text),
            "next_follow_up": self._get_text(self.next_follow_up_text),
        }

    def save_state(self) -> None:
        state = self._collect_state()
        try:
            self.state_path.write_text(json.dumps(state, indent=2), encoding="utf-8")
            self._append_log_entry(state)
            self.status_var.set(f"Saved checkpoint to {self.state_path}")
        except Exception as exc:
            self.status_var.set(f"Save failed: {exc}")
        self.refresh_ui()

    def reset_state(self) -> None:
        self.class_name_var.set("")
        self.learner_group_var.set("")
        self._set_text(self.bright_spots_text, "")
        self._set_text(self.watch_items_text, "")
        self._set_text(self.support_moves_text, "")
        self._set_text(self.next_follow_up_text, "")
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
        try:
            self.state_path.write_text("{}", encoding="utf-8")
        except Exception:
            pass
        self.refresh_ui()

    def refresh_ui(self) -> None:
        state = self._collect_state()
        completion = round((self._field_completion(state) / len(FIELD_KEYS)) * 100)
        watch_count = len(self._meaningful_lines(state["watch_items"]))
        summary = self._bridge_summary(state, watch_count)
        snapshot = self._bridge_snapshot(state)

        self.completion_var.set(f"{completion}%")
        self.watch_count_var.set(str(watch_count))
        self.bridge_mode_var.set("hosted" if self.bridge_status_path else "standalone")
        self._set_preview(self.summary_preview, summary)
        self._set_preview(self.snapshot_preview, snapshot)
        self._sync_bridge(summary, snapshot, completion, watch_count)
        self._sync_shared_state(state, summary, completion, watch_count)

    def _field_completion(self, state: dict) -> int:
        return sum(1 for key in FIELD_KEYS if state.get(key, "").strip())

    def _meaningful_lines(self, value: str) -> list[str]:
        return [line.strip() for line in value.splitlines() if line.strip()]

    def _bridge_summary(self, state: dict, watch_count: int) -> str:
        class_name = state["class_name"].strip() or "an unnamed class"
        group = state["learner_group"].strip() or "the current learner group"
        next_step = state["next_follow_up"].strip() or "decide the next follow-up"
        return (
            f"Teacher Notebook is tracking {group} in {class_name}. "
            f"Watch items logged: {watch_count}. "
            f"Next follow-up: {next_step}."
        )

    def _bridge_snapshot(self, state: dict) -> str:
        return "\n".join(
            [
                "# Teacher Notebook Snapshot",
                "",
                f"- Class or subject: {state['class_name'].strip() or 'not set'}",
                f"- Learner group: {state['learner_group'].strip() or 'not set'}",
                "",
                "## Bright spots",
                state["bright_spots"].strip() or "(empty)",
                "",
                "## Watch items",
                state["watch_items"].strip() or "(empty)",
                "",
                "## Support moves",
                state["support_moves"].strip() or "(empty)",
                "",
                "## Next follow-up",
                state["next_follow_up"].strip() or "(empty)",
            ]
        )

    def _sync_bridge(
        self, summary: str, snapshot: str, completion: int, watch_count: int
    ) -> None:
        if not self.bridge_status_path:
            return

        payload = {
            "module_id": MODULE_ID,
            "event_type": "suspend_rundown",
            "summary": summary,
            "snapshot": snapshot,
            "tags": ["teacher", "notes", "native_window", "python", "demo"],
            "payload": {
                "completion": completion,
                "watch_count": watch_count,
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
        self, state: dict, summary: str, completion: int, watch_count: int
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
                    "watch_count": watch_count,
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
                                self.class_name_var.set(fields.get("class_name", ""))
                                self.learner_group_var.set(fields.get("learner_group", ""))
                                self._set_text(
                                    self.bright_spots_text,
                                    fields.get("bright_spots", ""),
                                )
                                self._set_text(
                                    self.watch_items_text,
                                    fields.get("watch_items", ""),
                                )
                                self._set_text(
                                    self.support_moves_text,
                                    fields.get("support_moves", ""),
                                )
                                self._set_text(
                                    self.next_follow_up_text,
                                    fields.get("next_follow_up", ""),
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

    def _append_log_entry(self, state: dict) -> None:
        try:
            self.log_path.parent.mkdir(parents=True, exist_ok=True)
            timestamp = datetime.now().strftime("%Y-%m-%d %H:%M:%S")
            lines = [
                f"## {timestamp}",
                "",
                f"- Class: {state['class_name'].strip() or 'not set'}",
                f"- Learner group: {state['learner_group'].strip() or 'not set'}",
                f"- Watch items: {len(self._meaningful_lines(state['watch_items']))}",
                f"- Next follow-up: {state['next_follow_up'].strip() or 'not set'}",
                "",
            ]
            with self.log_path.open("a", encoding="utf-8") as handle:
                handle.write("\n".join(lines))
                handle.write("\n")
        except Exception:
            pass

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
    TeacherNotebookApp().run()
