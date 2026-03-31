#!/usr/bin/env python3
import argparse
import json
import pathlib
import shlex
import sys


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--policy", required=True)
    parser.add_argument("--tier", required=True)
    args = parser.parse_args()

    policy_path = pathlib.Path(args.policy)
    policy = json.loads(policy_path.read_text())
    tiers = policy.get("tiers", {})
    tier = tiers.get(args.tier)
    if tier is None:
      print(f"Unknown validation tier: {args.tier}", file=sys.stderr)
      return 1

    artifact_root = policy.get("artifact_root", "target/test-artifacts")
    command = tier["command"]
    print(f"artifact_root={artifact_root}")
    print(f"retention_days={tier['retention_days']}")
    print(f"upload_mode={tier['upload_mode']}")
    print(f"budget_guard_mode={tier['budget_guard_mode']}")
    print(f"command={shlex.quote(command)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
