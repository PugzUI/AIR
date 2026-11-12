import { useEffect, useRef, useState } from "react";
import { Button } from "@/components/nexvyn/button";
import { readConfig } from "./helpers";

type Props = {
  fleet: any;
  host: any;
  busy: boolean;
  run: (fn: () => any) => Promise<boolean>;
  created: () => void;
  initialProvider?: string;
};

function ConfigField({
  name,
  property,
  config,
}: {
  name: string;
  property: any;
  config: any;
}) {
  const value = config.values?.[name] ?? property.default;
  const required = (config.schema?.required || []).includes(name);
  const title = property.title || name;
  if (
    property.readOnly ||
    !["string", "number", "boolean", "array", "object"].includes(property.type)
  )
    return (
      <div className="field-row">
        <strong>{title}</strong>
        <span>
          {property.readOnly ? "Managed by host" : "Advanced host setting"}
          {required ? " · Required" : ""}
        </span>
        {property.description && <small>{property.description}</small>}
      </div>
    );
  const common = { className: "field", name: `config:${name}`, required };
  let input;
  if (
    property.type === "object" ||
    (property.type === "array" && property.items?.type !== "string")
  )
    input = (
      <textarea
        {...common}
        rows={4}
        defaultValue={JSON.stringify(
          value ?? (property.type === "array" ? [] : {}),
          null,
          2,
        )}
      />
    );
  else if (property.type === "array")
    input = (
      <textarea
        {...common}
        rows={3}
        defaultValue={Array.isArray(value) ? value.join("\n") : ""}
        placeholder="One entry per line"
      />
    );
  else if (Array.isArray(property.enum) && !property.enumDynamic) {
    const index = property.enum.findIndex((choice: any) =>
      Object.is(choice, value),
    );
    input = (
      <select {...common} defaultValue={index < 0 ? "" : String(index)}>
        <option value="">
          {required ? "Choose an option" : "Host default"}
        </option>
        {property.enum.map((choice: any, i: number) => (
          <option key={i} value={i}>
            {property.enumLabels?.[i] || String(choice)}
          </option>
        ))}
      </select>
    );
  } else if (property.type === "boolean")
    input = (
      <select
        {...common}
        defaultValue={value === undefined ? "" : String(value)}
      >
        <option value="">
          {required ? "Choose an option" : "Host default"}
        </option>
        <option value="true">Yes</option>
        <option value="false">No</option>
      </select>
    );
  else
    input = (
      <>
        <input
          {...common}
          defaultValue={value == null ? "" : String(value)}
          type={property.type === "number" ? "number" : "text"}
          step={property.type === "number" ? "any" : undefined}
          autoComplete="off"
          list={
            property.enumDynamic && property.enum?.length
              ? `choices-${name}`
              : undefined
          }
        />
        {property.enumDynamic && property.enum?.length > 0 && (
          <datalist id={`choices-${name}`}>
            {property.enum.map((choice: any, i: number) => (
              <option
                key={i}
                value={String(choice)}
                label={property.enumLabels?.[i] || String(choice)}
              />
            ))}
          </datalist>
        )}
      </>
    );
  return (
    <label>
      {title}
      {input}
      {property.description && <small>{property.description}</small>}
    </label>
  );
}

export function CreateSession({
  fleet,
  host,
  busy,
  run,
  created,
  initialProvider,
}: Props) {
  const form = useRef<HTMLFormElement>(null);
  const revision = useRef(0);
  const [config, setConfig] = useState<any>(null);
  const [configKey, setConfigKey] = useState("");
  const [generation, setGeneration] = useState(0);
  const live = fleet?.eventStreamHealthy && host?.state.status === "connected";
  function params(data: FormData) {
    return {
      hostId: host.id,
      provider: String(data.get("provider") || ""),
      directory: String(data.get("directory") || ""),
    };
  }
  async function resolve(reset = false) {
    if (!form.current || !live) return;
    const data = new FormData(form.current);
    const request = params(data);
    if (!request.provider) return;
    const key = JSON.stringify([
      request.hostId,
      request.provider,
      request.directory,
    ]);
    const currentRevision = ++revision.current;
    await run(async () => {
      const values = reset ? {} : readConfig(data, config);
      const result = await fleet.resolveSessionConfig({
        ...request,
        config: values,
      });
      if (revision.current === currentRevision) {
        setConfig(result);
        setConfigKey(key);
        setGeneration((n) => n + 1);
      }
    });
  }
  useEffect(() => {
    if (!form.current || !live) return;
    let cancelled = false;
    const currentRevision = ++revision.current;
    const data = new FormData(form.current);
    const request = {
      hostId: host.id,
      provider: String(data.get("provider") || ""),
      directory: String(data.get("directory") || ""),
    };
    if (request.provider)
      void run(async () => {
        const result = await fleet.resolveSessionConfig(request);
        if (!cancelled && revision.current === currentRevision) {
          setConfig(result);
          setConfigKey(
            JSON.stringify([
              request.hostId,
              request.provider,
              request.directory,
            ]),
          );
          setGeneration((n) => n + 1);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [fleet, host.id, live, run]);
  return (
    <form
      className="panel-section"
      ref={form}
      onChange={(event) => {
        const target = event.target;
        if (!(
          target instanceof HTMLInputElement ||
          target instanceof HTMLSelectElement ||
          target instanceof HTMLTextAreaElement
        ))
          return;
        if (target.name === "provider") {
          setConfig(null);
          setConfigKey("");
          void resolve(true);
        } else if (target.name === "directory") {
          setConfig(null);
          setConfigKey("");
          revision.current++;
        } else if (
          target.name.startsWith("config:") &&
          target.tagName === "SELECT"
        )
          void resolve();
      }}
      onSubmit={(event) => {
        event.preventDefault();
        const data = new FormData(event.currentTarget);
        void run(async () => {
          const request = params(data);
          const key = JSON.stringify([
            request.hostId,
            request.provider,
            request.directory,
          ]);
          const values =
            config && configKey === key ? readConfig(data, config) : {};
          const result = await fleet.resolveSessionConfig({
            ...request,
            config: values,
          });
          setConfig(result);
          setConfigKey(key);
          setGeneration((n) => n + 1);
          const resolved = { ...(result.values || {}), ...values };
          const required = result.schema?.required || [];
          if (
            required.some(
              (field: string) =>
                resolved[field] == null ||
                resolved[field] === "" ||
                (Array.isArray(resolved[field]) &&
                  result.schema.properties?.[field]?.minItems >
                    resolved[field].length),
            )
          )
            throw Error("Complete the host-required session options");
          await fleet.createSession({
            ...request,
            config: resolved,
            trust: data.get("trustWorkspace") === "on",
          });
          created();
        });
      }}
    >
      <label>
        Agent
        <select
          className="field"
          name="provider"
          required
          defaultValue={
            host.agents?.some((a: any) => a.provider === initialProvider)
              ? initialProvider
              : host.agents?.[0]?.provider || ""
          }
        >
          {(host.agents || []).map((agent: any) => (
            <option key={agent.provider} value={agent.provider}>
              {agent.displayName || agent.provider}
            </option>
          ))}
        </select>
      </label>
      <label>
        Workspace (file URI)
        <input
          className="field"
          name="directory"
          type="text"
          spellCheck={false}
          placeholder="file:///path/to/project"
          defaultValue={host.defaultDirectory || ""}
        />
      </label>
      <label className="field-row">
        <input type="checkbox" name="trustWorkspace" />
        <span>
          Trust this workspace for this browser tab{" "}
          <small>
            Allow this agent to load repository instructions and hooks. Only
            select projects you trust.
          </small>
        </span>
      </label>
      <div className="action-row">
        <strong>Host session options</strong>
        <Button
          type="button"
          variant="ghost"
          disabled={busy || !live || !host.agents?.length}
          onClick={() => void resolve(!config)}
        >
          Refresh options
        </Button>
      </div>
      {config ? (
        <div key={generation} className="config-fields">
          {Object.entries<any>(config.schema?.properties || {}).map(
            ([name, property]) => (
              <ConfigField
                key={name}
                name={name}
                property={property}
                config={config}
              />
            ),
          )}
          {!Object.keys(config.schema?.properties || {}).length && (
            <p className="muted">No additional host options.</p>
          )}
        </div>
      ) : (
        <p className="muted">
          Host options not loaded. Refresh to request them.
        </p>
      )}
      {!host.agents?.length && (
        <p>The selected host currently advertises no agents.</p>
      )}
      <Button
        type="submit"
        variant="ghost"
        className="action-primary"
        disabled={busy || !live || !host.agents?.length}
      >
        Create session
      </Button>
    </form>
  );
}
