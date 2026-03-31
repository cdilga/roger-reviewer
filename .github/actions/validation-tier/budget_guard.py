#!/usr/bin/env python3
import argparse
import json
import pathlib
import sys


def load_budget(workspace: pathlib.Path) -> tuple[pathlib.Path | None, dict | None]:
    candidates = [
        workspace / "docs" / "AUTOMATED_E2E_BUDGET.json",
        workspace / "docs" / "E2E_BUDGET.json",
    ]
    for candidate in candidates:
        if candidate.exists():
            return candidate, json.loads(candidate.read_text())
    return None, None


def emit_notice(message: str) -> None:
    print(message)
    summary = pathlib.Path.cwd() / pathlib.Path(".github-step-summary")
    if summary.exists():
        with summary.open("a", encoding="utf-8") as handle:
            handle.write(message + "\n")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--workspace", required=True)
    parser.add_argument("--tier", required=True)
    parser.add_argument("--mode", required=True, choices=["off", "warn", "enforce"])
    args = parser.parse_args()

    if args.mode == "off":
        return 0

    budget_path, budget = load_budget(pathlib.Path(args.workspace))
    if budget is None or budget_path is None:
        message = f"::warning title=Roger validation budget::No budget file found for tier {args.tier}; skipping guard."
        print(message)
        return 0

    if "blessed_automated_e2e_budget" in budget:
        allowed = int(budget["blessed_automated_e2e_budget"])
        current = int(budget.get("current_planned_blessed_automated_e2e_count", allowed))
        required_fields = budget.get("required_justification_fields_for_growth", [])
        justification_source = budget
    else:
        allowed = int(budget.get("blessed_automated_e2e_count", 0))
        current = len(budget.get("blessed_tests", []))
        required_fields = [
            "product_promise_defended",
            "why_lower_layer_is_insufficient",
            "boundaries_crossed",
            "estimated_maintenance_cost",
            "why_not_acceptance_or_release_smoke",
        ]
        justification_source = budget.get("growth_justification", {})

    missing_fields = []
    if current > allowed:
        for field in required_fields:
            value = justification_source.get(field)
            if value in (None, "", [], {}):
                missing_fields.append(field)

    if current <= allowed:
        print(f"Roger E2E budget OK for tier {args.tier}: {current}/{allowed} via {budget_path.relative_to(args.workspace)}")
        return 0

    message = (
        f"ROGER_E2E_BUDGET_EXCEEDED: tier={args.tier} current={current} allowed={allowed} "
        f"budget_file={budget_path.relative_to(args.workspace)} missing_justification_fields={','.join(missing_fields) or 'none'}"
    )

    if args.mode == "warn":
        print(f"::warning title=Roger validation budget::{message}")
        return 0

    print(f"::error title=Roger validation budget::{message}")
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
