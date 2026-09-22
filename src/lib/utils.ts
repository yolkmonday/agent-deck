export type ClassValue =
  | string
  | number
  | null
  | undefined
  | false
  | ClassValue[]
  | Record<string, boolean | null | undefined>;

/**
 * Joins class names. The shadcn components expect the usual `cn` helper; this one
 * stays dependency-free because nothing here needs conflict resolution — callers
 * only append layout classes to a component's own base.
 */
export const cn = (...inputs: ClassValue[]): string => {
  const out: string[] = [];
  const push = (value: ClassValue): void => {
    if (typeof value === "number") {
      out.push(String(value));
      return;
    }
    if (!value) return;
    if (typeof value === "string") {
      out.push(value);
      return;
    }
    if (Array.isArray(value)) {
      value.forEach(push);
      return;
    }
    for (const [name, on] of Object.entries(value)) {
      if (on) out.push(name);
    }
  };
  inputs.forEach(push);
  return out.join(" ");
};
