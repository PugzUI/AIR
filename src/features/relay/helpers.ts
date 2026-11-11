export const safeError = (error: unknown) =>
  String(
    error instanceof Error ? error.message : error || "Unknown error",
  ).replace(/([?&]tkn=)[^&#\s"'<>]+/gi, "$1[redacted]");

export const tail = (value: unknown) =>
  String(value || "")
    .split("/")
    .filter(Boolean)
    .at(-1) || String(value || "—");
export const labelText = (value: any) =>
  typeof value === "string" ? value : value?.markdown || "";
export const when = (value: any) =>
  value && !Number.isNaN(new Date(value).getTime())
    ? new Date(value).toLocaleString()
    : "Not reported";

export function readConfig(data: FormData, config: any) {
  const values: Record<string, any> = { ...(config?.values || {}) };
  for (const [key, property] of Object.entries<any>(
    config?.schema?.properties || {},
  )) {
    if (property.readOnly) continue;
    const raw = data.get(`config:${key}`);
    if (raw === null) continue;
    const text = String(raw);
    if (
      property.type === "object" ||
      (property.type === "array" && property.items?.type !== "string")
    ) {
      if (!text.trim()) continue;
      let parsed;
      try {
        parsed = JSON.parse(text);
      } catch {
        throw Error(`Invalid JSON for host option ${property.title || key}`);
      }
      if (
        property.type === "object"
          ? !parsed || typeof parsed !== "object" || Array.isArray(parsed)
          : !Array.isArray(parsed)
      )
        throw Error(`Invalid type for host option ${property.title || key}`);
      values[key] = parsed;
    } else if (property.type === "array")
      values[key] = text
        .split(/\r?\n/)
        .map((v) => v.trim())
        .filter(Boolean);
    else if (text === "") continue;
    else if (property.enum?.length && !property.enumDynamic) {
      const index = Number(text);
      if (
        !Number.isInteger(index) ||
        index < 0 ||
        index >= property.enum.length
      )
        throw Error(`Invalid host option ${property.title || key}`);
      values[key] = property.enum[index];
    } else if (property.type === "boolean") values[key] = text === "true";
    else if (property.type === "number") {
      const value = Number(text);
      if (!Number.isFinite(value))
        throw Error(`Invalid host option ${property.title || key}`);
      values[key] = value;
    } else if (property.type === "string") values[key] = text;
  }
  return values;
}

export function readAnswer(question: any, data: FormData) {
  const values = data.getAll(question.id).map(String).filter(Boolean);
  const kind = question.kind;
  const freeform =
    question.allowFreeformInput &&
    (kind === "single-select" || kind === "multi-select")
      ? String(data.get(`${question.id}:freeform`) || "").trim()
      : "";
  if (!values.length && !freeform && question.required)
    throw Error(`Complete ${question.title || question.message}`);
  if (!values.length && !freeform) return { state: "skipped" };
  let value;
  if (kind === "boolean")
    value = { kind: "boolean", value: values[0] === "true" };
  else if (kind === "number" || kind === "integer") {
    const number = Number(values[0]);
    if (
      !Number.isFinite(number) ||
      (kind === "integer" && !Number.isInteger(number))
    )
      throw Error(`Invalid number for ${question.id}`);
    value = { kind: "number", value: number };
  } else if (kind === "single-select")
    value = {
      kind: "selected",
      value: values[0] || "",
      ...(freeform ? { freeformValues: [freeform] } : {}),
    };
  else if (kind === "multi-select")
    value = {
      kind: "selected-many",
      value: values,
      ...(freeform ? { freeformValues: [freeform] } : {}),
    };
  else value = { kind: "text", value: values[0] };
  return { state: "submitted", value };
}

export function topologyFocusId(
  nodes: { id: string }[],
  focused?: string,
  selected?: string,
) {
  return (
    [focused, selected].find((id) => nodes.some((node) => node.id === id)) ||
    nodes[0]?.id
  );
}
