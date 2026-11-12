import { useState } from "react";
import { RefreshCw, Send, X } from "lucide-react";
import { Button } from "@/components/nexvyn/button";
import {
  TabsSubtle,
  TabsSubtleItem,
  TabsSubtlePanel,
} from "@/components/nexvyn/tabs-subtle";
import { attentionFor, sessionState } from "../../fleet.mjs";
import { labelText, readAnswer, safeError, tail, when } from "./helpers";

type Props = {
  fleet: any;
  selected: any;
  summary: any;
  host: any;
  busy: boolean;
  run: (fn: () => any) => Promise<boolean>;
};

function formatUnifiedDiff(before?: string | null, after?: string | null) {
  if (before != null && after != null) {
    const a = before.split("\n");
    const b = after.split("\n");
    let start = 0;
    while (start < a.length && start < b.length && a[start] === b[start]) {
      start++;
    }
    let endA = a.length - 1;
    let endB = b.length - 1;
    while (endA >= start && endB >= start && a[endA] === b[endB]) {
      endA--;
      endB--;
    }

    const lines: string[] = [];
    for (let i = 0; i < start; i++) {
      lines.push(`  ${a[i]}`);
    }

    const midA = a.slice(start, endA + 1);
    const midB = b.slice(start, endB + 1);

    if (
      midA.length > 0 &&
      midB.length > 0 &&
      midA.length <= 400 &&
      midB.length <= 400
    ) {
      const m = midA.length;
      const n = midB.length;
      const dp = Array.from({ length: m + 1 }, () => new Int32Array(n + 1));
      for (let i = 1; i <= m; i++) {
        for (let j = 1; j <= n; j++) {
          if (midA[i - 1] === midB[j - 1]) {
            dp[i][j] = dp[i - 1][j - 1] + 1;
          } else {
            dp[i][j] = Math.max(dp[i - 1][j], dp[i][j - 1]);
          }
        }
      }
      const diff: string[] = [];
      let i = m;
      let j = n;
      while (i > 0 || j > 0) {
        if (i > 0 && j > 0 && midA[i - 1] === midB[j - 1]) {
          diff.push(`  ${midA[i - 1]}`);
          i--;
          j--;
        } else if (j > 0 && (i === 0 || dp[i][j - 1] >= dp[i - 1][j])) {
          diff.push(`+ ${midB[j - 1]}`);
          j--;
        } else if (i > 0 && (j === 0 || dp[i][j - 1] < dp[i - 1][j])) {
          diff.push(`- ${midA[i - 1]}`);
          i--;
        }
      }
      diff.reverse();
      lines.push(...diff);
    } else {
      for (const line of midA) {
        lines.push(`- ${line}`);
      }
      for (const line of midB) {
        lines.push(`+ ${line}`);
      }
    }

    for (let i = endA + 1; i < a.length; i++) {
      lines.push(`  ${a[i]}`);
    }

    return <pre className="diff-preview">{lines.join("\n")}</pre>;
  }

  return (
    <>
      <h4>Before</h4>
      <pre>{before ?? "No before content supplied."}</pre>
      <h4>After</h4>
      <pre>{after ?? "No after content supplied."}</pre>
    </>
  );
}

function Attention({
  entry,
  live,
  run,
  fleet,
}: {
  entry: any;
  live: boolean;
  run: Props["run"];
  fleet: any;
}) {
  if (entry.kind === "toolConfirmation") {
    const call = entry.toolCall;
    const options =
      call.status === "pending-result-confirmation"
        ? []
        : (call.options || []).filter((option: any) =>
            ["approve", "deny"].includes(option.kind),
          );
    return (
      <div className="panel-section">
        <strong>
          {labelText(call.confirmationTitle) ||
            call.displayName ||
            "Tool confirmation"}
        </strong>
        <p>
          {call.intention ||
            labelText(call.invocationMessage) ||
            call.toolName ||
            "Review this tool request"}
        </p>
        <details>
          <summary>Tool parameters</summary>
          <pre>{JSON.stringify(call.toolInput ?? {}, null, 2)}</pre>
        </details>
        <div className="action-row">
          {options.length ? (
            options.map((option: any) => (
              <Button
                key={option.id}
                variant={option.kind === "approve" ? "ghost" : "outline"}
                className={option.kind === "approve" ? "action-primary" : ""}
                disabled={!live}
                onClick={() =>
                  void run(() =>
                    fleet.confirmTool(
                      entry,
                      option.kind === "approve",
                      option.id,
                    ),
                  )
                }
              >
                {option.label}
              </Button>
            ))
          ) : (
            <>
              <Button
                variant="ghost"
                className="action-primary"
                disabled={
                  !live ||
                  (call.status !== "pending-result-confirmation" &&
                    !!call.options?.length)
                }
                onClick={() => void run(() => fleet.confirmTool(entry, true))}
              >
                {call.status === "pending-result-confirmation"
                  ? "Accept result"
                  : "Approve once"}
              </Button>
              <Button
                variant="ghost"
                disabled={
                  !live ||
                  (call.status !== "pending-result-confirmation" &&
                    !!call.options?.length)
                }
                onClick={() => void run(() => fleet.confirmTool(entry, false))}
              >
                {call.status === "pending-result-confirmation"
                  ? "Reject result"
                  : "Deny"}
              </Button>
            </>
          )}
        </div>
      </div>
    );
  }
  if (entry.kind === "chatInput") {
    let link = "";
    try {
      const url = new URL(entry.request.url);
      if (["http:", "https:"].includes(url.protocol)) link = url.href;
    } catch {
      /* no safe link */
    }
    return (
      <form
        className="panel-section"
        onSubmit={(event) => {
          event.preventDefault();
          const data = new FormData(event.currentTarget);
          void run(() =>
            fleet.answerInput(
              entry,
              "accept",
              Object.fromEntries(
                (entry.request.questions || []).map((q: any) => [
                  q.id,
                  readAnswer(q, data),
                ]),
              ),
            ),
          );
        }}
      >
        <strong>{entry.request.message || "Agent requires input"}</strong>
        {link && (
          <a href={link} target="_blank" rel="noopener noreferrer">
            Review requested link ↗
          </a>
        )}
        {(entry.request.questions || []).map((q: any) => (
          <fieldset key={q.id}>
            <legend>{q.title || q.message}</legend>
            {q.kind === "boolean" ? (
              <select
                className="field"
                name={q.id}
                defaultValue={
                  q.defaultValue === undefined ? "" : String(q.defaultValue)
                }
                required={q.required}
              >
                <option value="">Choose an answer</option>
                <option value="true">Yes</option>
                <option value="false">No</option>
              </select>
            ) : q.kind === "single-select" || q.kind === "multi-select" ? (
              <>
                <select
                  className="field"
                  name={q.id}
                  multiple={q.kind === "multi-select"}
                  required={q.required && !q.allowFreeformInput}
                >
                  {q.kind === "single-select" && (
                    <option value="">Choose an option</option>
                  )}
                  {(q.options || []).map((o: any) => (
                    <option key={o.id} value={o.id}>
                      {o.label}
                    </option>
                  ))}
                </select>
                {q.allowFreeformInput && (
                  <label>
                    Other response
                    <input
                      className="field"
                      name={`${q.id}:freeform`}
                      autoComplete="off"
                    />
                  </label>
                )}
              </>
            ) : (
              <input
                className="field"
                name={q.id}
                type={
                  q.kind === "integer" || q.kind === "number"
                    ? "number"
                    : "text"
                }
                step={q.kind === "number" ? "any" : "1"}
                min={
                  q.kind === "number" || q.kind === "integer"
                    ? q.min
                    : undefined
                }
                max={
                  q.kind === "number" || q.kind === "integer"
                    ? q.max
                    : undefined
                }
                minLength={q.kind === "text" ? q.min : undefined}
                maxLength={q.kind === "text" ? q.max : undefined}
                defaultValue={
                  q.defaultValue == null ? "" : String(q.defaultValue)
                }
                required={q.required}
              />
            )}
          </fieldset>
        ))}
        <div className="action-row">
          <Button
            variant="ghost"
            className="action-primary"
            type="submit"
            disabled={!live}
          >
            Answer
          </Button>
          <Button
            variant="ghost"
            type="button"
            disabled={!live}
            onClick={() => void run(() => fleet.answerInput(entry, "decline"))}
          >
            Decline
          </Button>
        </div>
      </form>
    );
  }
  return (
    <div className="panel-section">
      <strong>{entry.kind}</strong>
      <p>Resolve this request in the agent host.</p>
    </div>
  );
}

export function SessionInspector({
  fleet,
  selected,
  summary,
  host,
  busy,
  run,
}: Props) {
  const [tab, setTab] = useState("chat");
  const [model, setModel] = useState("");
  const [preview, setPreview] = useState<any>(null);
  const [confirmDispose, setConfirmDispose] = useState(false);
  const detail = fleet?.session;
  const chat = fleet?.chat;
  const connected =
    !!fleet?.eventStreamHealthy &&
    host?.state?.status === "connected";
  const live = connected && !busy;
  const policy = fleet?.chatPromptPolicy || {
    allowsInput: false,
    reason: "Chat is not ready",
  };
  const models = fleet?.modelsFor(
    selected.hostId,
    detail?.provider || summary?.provider,
  ) || [];
  const changesets = fleet?.availableChangesets() || [];
  const changeset = fleet?.changeset;
  const changes = fleet?.changes;
  const chats = (detail?.chats || []).filter(
    (c: any) => c.interactivity !== "hidden",
  );
  const slotError = fleet?.slots.session?.error;
  const turns = chat?.turns || [];
  const template =
    changeset?.sourceTemplate || changesets[0]?.sourceTemplate || "";
  const turnIds = turns.filter((turn: any) => typeof turn.id === "string");
  function selectTurn(field: string, value: string) {
    const params = { ...fleet.changesetParams, [field]: value };
    const entry = fleet
      .availableChangesets(params)
      .find((item: any) => item.sourceTemplate === changeset?.sourceTemplate);
    if (entry) {
      setPreview(null);
      void run(() =>
        fleet.selectChangeset(entry.uriTemplate, undefined, params),
      );
    }
  }
  function inspect(file: any) {
    if (preview?.fileId === file.id) {
      setPreview(null);
      return;
    }
    const identity = {
      fileId: file.id,
      changes,
      changesetUri: fleet.slots.changes?.uri,
      hostId: selected.hostId,
      sessionUri: selected.uri,
    };
    setPreview({ ...identity, loading: true });
    void (async () => {
      try {
        const data = await fleet.readEdit(file.id);
        setPreview((current: any) =>
          current?.changes === identity.changes &&
          current?.changesetUri === identity.changesetUri &&
          current?.fileId === file.id
            ? { ...identity, ...data }
            : current,
        );
      } catch (error) {
        setPreview((current: any) =>
          current?.changes === identity.changes &&
          current?.changesetUri === identity.changesetUri &&
          current?.fileId === file.id
            ? { ...identity, error: safeError(error) }
            : current,
        );
      }
    })();
  }
  const shownPreview =
    preview &&
    preview.changes === changes &&
    preview.changesetUri === fleet?.slots.changes?.uri &&
    preview.hostId === selected.hostId &&
    preview.sessionUri === selected.uri
      ? preview
      : null;
  return (
    <>
      <div className="panel-section">
        <span className="muted">
          {host?.label || selected.hostId} /{" "}
          {summary?.provider || detail?.provider || "SESSION"}
        </span>
        <h2>
          {summary?.title ||
            detail?.title ||
            (slotError ? "Session unavailable" : "Loading session")}
        </h2>
        <p>
          {sessionState(
            detail?.lifecycle === "failed" ? detail : summary || detail,
            connected,
          )}
        </p>
        {!connected && (
          <p role="status">
            Host disconnected or unavailable. Published state may be stale;
            controls are unavailable.
          </p>
        )}
        {slotError && (
          <p role="alert">
            {safeError(slotError)}. The host may have restored this session
            under a new ID.
          </p>
        )}
        {detail?.lifecycle === "failed" && (
          <p role="alert">
            {safeError(
              detail.creationError?.message ||
                "The host could not initialize this session.",
            )}
          </p>
        )}
      </div>
      <TabsSubtle
        value={tab}
        onValueChange={setTab}
        idPrefix="relay-session"
        aria-label="Session views"
      >
        <TabsSubtleItem value="chat" label="Conversation" />
        <TabsSubtleItem value="files" label="Files" />
        <TabsSubtleItem value="info" label="Info" />
        <TabsSubtlePanel value="chat">
          <section className="panel-section">
            {attentionFor(detail).map((entry: any) => (
              <Attention
                key={entry.id}
                entry={entry}
                live={live}
                run={run}
                fleet={fleet}
              />
            ))}
            {chats.length > 1 && (
              <label>
                Chat
                <select
                  className="field"
                  value={fleet?.slots.chat?.uri || ""}
                  disabled={!live}
                  onChange={(event) =>
                    void run(() => fleet.selectChat(event.target.value))
                  }
                >
                  {chats.map((c: any) => (
                    <option key={c.resource} value={c.resource}>
                      {c.title || c.resource}
                    </option>
                  ))}
                </select>
              </label>
            )}
            <div className="action-row">
              <strong>Conversation</strong>
              <Button
                variant="ghost"
                disabled={!live}
                onClick={() => void run(() => fleet.refreshSelection())}
              >
                <RefreshCw /> Refresh
              </Button>
            </div>
            <div role="log" aria-live="off">
              {chat?.turnsNextCursor && (
                <Button
                  variant="ghost"
                  disabled={!live}
                  onClick={() => void run(() => fleet.loadOlderTurns())}
                >
                  Load earlier turns
                </Button>
              )}
              {[...turns, ...(chat?.activeTurn ? [chat.activeTurn] : [])].map(
                (turn: any, i: number) => (
                  <article className="session-message" key={turn.id || i}>
                    <small>Prompt · {when(turn.startedAt)}</small>
                    <p>{turn.message?.text || "—"}</p>
                    {(turn.responseParts || []).map(
                      (part: any, index: number) =>
                        part.kind === "markdown" ? (
                          <pre key={index}>{part.content}</pre>
                        ) : part.kind === "toolCall" ? (
                          <p key={index}>
                            Tool ·{" "}
                            {part.toolCall?.displayName ||
                              part.toolCall?.toolName}{" "}
                            · {part.toolCall?.status}
                          </p>
                        ) : part.kind === "error" ? (
                          <p role="alert" key={index}>
                            {safeError(part.error?.message || "Agent error")}
                          </p>
                        ) : part.kind === "inputRequest" ? (
                          <p key={index}>
                            Agent requested input · {part.response || "pending"}
                          </p>
                        ) : null,
                    )}
                    <small>
                      {turn.state === "complete" && !turn.responseParts?.length
                        ? "Host reports complete · no response recorded"
                        : turn.state || "In progress"}
                      {turn.usage?.totalTokens
                        ? ` · ${turn.usage.totalTokens} tokens`
                        : ""}
                    </small>
                  </article>
                ),
              )}
              {!turns.length && !chat?.activeTurn && (
                <p className="muted">
                  {chat
                    ? "No turns in this chat."
                    : fleet?.slots.chat?.error
                      ? safeError(fleet.slots.chat.error)
                      : "Awaiting host chat."}
                </p>
              )}
            </div>
            {chat?.queuedMessages?.length > 0 && (
              <p>{chat.queuedMessages.length} queued turn(s)</p>
            )}
            {fleet?.waiting.size > 0 && (
              <p role="status">
                Awaiting host echo for {fleet.waiting.size} submission(s). Do
                not retry until confirmed.
              </p>
            )}
            {chat && !policy.allowsInput && (
              <p role="status">{policy.reason}</p>
            )}
            <form
              className="composer"
              onSubmit={(event) => {
                event.preventDefault();
                const form = event.currentTarget;
                const input = form.elements.namedItem(
                  "prompt",
                ) as HTMLTextAreaElement;
                void run(() => {
                  fleet.queueTurn(input.value, model || undefined);
                  input.value = "";
                });
              }}
            >
              <label>
                Message / queued turn{" "}
                <small className="muted">Ctrl+Enter to queue</small>
                <textarea
                  className="field"
                  name="prompt"
                  rows={4}
                  required
                  disabled={!live || !chat || !policy.allowsInput}
                  placeholder="Describe the work to perform…"
                  onKeyDown={(event) => {
                    if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
                      event.preventDefault();
                      event.currentTarget.form?.requestSubmit();
                    }
                  }}
                />
              </label>
              {models.length > 0 && (
                <label>
                  Model
                  <select
                    className="field"
                    value={models.some((m: any) => m.id === model) ? model : ""}
                    disabled={!live || !policy.allowsInput}
                    onChange={(event) => setModel(event.target.value)}
                  >
                    <option value="">Host default</option>
                    {models.map((m: any) => (
                      <option
                        key={m.id}
                        value={m.id}
                        disabled={m.policyState === "disabled"}
                      >
                        {m.name || m.id}
                        {m.policyState === "unconfigured"
                          ? " · Not configured"
                          : ""}
                      </option>
                    ))}
                  </select>
                </label>
              )}
              <div className="action-row">
                <Button
                  type="button"
                  variant="ghost"
                  disabled={!live || !chat?.activeTurn}
                  onClick={() => void run(() => fleet.cancelTurn())}
                >
                  <X /> Cancel turn
                </Button>
                <Button
                  type="submit"
                  variant="ghost"
                  className="action-primary"
                  disabled={!live || !chat || !policy.allowsInput}
                >
                  Queue turn <Send />
                </Button>
              </div>
            </form>
          </section>
        </TabsSubtlePanel>
        <TabsSubtlePanel value="files">
          <section className="panel-section">
            <h3>Files / changes</h3>
            {!changesets.length ? (
              <p className="muted">
                {!fleet?.changesetCatalog().length
                  ? "Host advertises no file changes."
                  : "Complete a turn to inspect host-advertised changes."}
              </p>
            ) : (
              <>
                {changesets.length > 1 && (
                  <label>
                    Changeset
                    <select
                      className="field"
                      value={changeset?.uriTemplate || ""}
                      disabled={!live}
                      onChange={(event) => {
                        setPreview(null);
                        void run(() =>
                          fleet.selectChangeset(event.target.value),
                        );
                      }}
                    >
                      {changesets.map((entry: any) => (
                        <option
                          key={entry.uriTemplate}
                          value={entry.uriTemplate}
                        >
                          {entry.label}
                        </option>
                      ))}
                    </select>
                  </label>
                )}
                {["turnId", "originalTurnId", "modifiedTurnId"]
                  .filter(
                    (field) =>
                      template.includes(`{${field}}`) && turnIds.length,
                  )
                  .map((field) => (
                    <label key={field}>
                      {field === "turnId"
                        ? "Turn"
                        : field === "originalTurnId"
                          ? "From"
                          : "To"}
                      <select
                        className="field"
                        disabled={!live}
                        value={
                          fleet?.changesetParams[field] ||
                          (field === "originalTurnId"
                            ? turnIds.at(-2)?.id
                            : turnIds.at(-1)?.id) ||
                          ""
                        }
                        onChange={(event) =>
                          selectTurn(field, event.target.value)
                        }
                      >
                        {turnIds.map((turn: any, i: number) => (
                          <option key={turn.id} value={turn.id}>
                            {i + 1} · {turn.message?.text}
                          </option>
                        ))}
                      </select>
                    </label>
                  ))}
                <p>
                  {changeset?.label} · {changes?.status || "Awaiting host"}
                </p>
                {fleet?.slots.changes?.error && (
                  <p role="alert">{safeError(fleet.slots.changes.error)}</p>
                )}
                {changes?.status === "error" && (
                  <p role="alert">
                    {safeError(
                      changes.error?.message || "Changeset failed to load",
                    )}
                  </p>
                )}
                {changes?.status === "recomputing" && (
                  <p>Recomputing; displaying previous result.</p>
                )}
                {(changes?.files || []).map((file: any) => (
                  <article className="session-message" key={file.id}>
                    <strong>
                      {tail(
                        file.edit?.after?.uri ||
                          file.edit?.before?.uri ||
                          file.id,
                      )}
                    </strong>
                    <p>
                      {file.edit?.after?.uri ||
                        file.edit?.before?.uri ||
                        file.id}
                    </p>
                    <Button
                      variant="ghost"
                      disabled={!live}
                      onClick={() => inspect(file)}
                    >
                      {shownPreview?.fileId === file.id
                        ? "Close preview"
                        : "Inspect file"}
                    </Button>
                    {shownPreview?.fileId === file.id && (
                      <div>
                        {shownPreview.loading ? (
                          <p>Reading file content…</p>
                        ) : shownPreview.error ? (
                          <p role="alert">{shownPreview.error}</p>
                        ) : (
                          formatUnifiedDiff(
                            shownPreview.before,
                            shownPreview.after,
                          )
                        )}
                      </div>
                    )}
                    {changeset?.capabilities &&
                      Object.hasOwn(changeset.capabilities, "review") && (
                        <label>
                          <input
                            type="checkbox"
                            checked={!!file.reviewed}
                            disabled={!live}
                            onChange={(event) =>
                              void run(() =>
                                fleet.reviewFile(file.id, event.target.checked),
                              )
                            }
                          />{" "}
                          Reviewed
                        </label>
                      )}
                  </article>
                ))}
                {changes?.status === "ready" && !changes.files?.length && (
                  <p>No changed files reported.</p>
                )}
              </>
            )}
          </section>
        </TabsSubtlePanel>
        <TabsSubtlePanel value="info">
          <section className="panel-section">
            <dl>
              <dt>Session</dt>
              <dd>{selected.uri}</dd>
              <dt>Workspace</dt>
              <dd>
                {detail?.workingDirectories?.[0] ||
                  summary?.workingDirectories?.[0] ||
                  "—"}
              </dd>
              <dt>Last activity</dt>
              <dd>{when(summary?.modifiedAt)}</dd>
            </dl>
            {!confirmDispose ? (
              <Button
                variant="ghost"
                destructive
                disabled={!live || !!slotError}
                onClick={() => setConfirmDispose(true)}
              >
                Dispose session
              </Button>
            ) : (
              <>
                <p role="alert">
                  Dispose this AHP session? This cannot be undone.
                </p>
                <div className="action-row">
                  <Button
                    variant="ghost"
                    destructive
                    disabled={!live || !!slotError}
                    onClick={() => {
                      setConfirmDispose(false);
                      void run(() => fleet.disposeSession());
                    }}
                  >
                    Confirm dispose
                  </Button>
                  <Button
                    variant="ghost"
                    onClick={() => setConfirmDispose(false)}
                  >
                    Cancel
                  </Button>
                </div>
              </>
            )}
          </section>
        </TabsSubtlePanel>
      </TabsSubtle>
    </>
  );
}
