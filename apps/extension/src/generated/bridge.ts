// Generated bridge contract snapshot for extension-side typing.
// Source of truth: packages/bridge/src/lib.rs (BridgeLaunchIntent / BridgeResponse).

export type BridgeAction =
  | 'start_review'
  | 'resume_review'
  | 'show_findings'
  | 'refresh_review';

export interface BridgeLaunchIntent {
  action: BridgeAction;
  owner: string;
  repo: string;
  pr_number: number;
  head_ref?: string;
  instance?: string;
}

export interface BridgeResponse {
  ok: boolean;
  action: string;
  message: string;
  session_id?: string;
  guidance?: string;
}
