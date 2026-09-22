import { Icon } from "@iconify/react";
import { agentIcon, fallbackTile, modelIcon, providerIcon } from "@/lib/icons";
import type { Agent } from "@/lib/types";

const FALLBACK_GLYPH = "lucide:cpu";

export const BrandIcon = ({
  name,
  fallbackKey,
  size = 16,
  className,
}: {
  name: string | null;
  fallbackKey: string;
  size?: number;
  className?: string;
}) => {
  if (name !== null) return <Icon icon={name} width={size} height={size} className={className} />;

  const { color, initials } = fallbackTile(fallbackKey);
  const inner = Math.max(8, Math.round(size * 0.6));
  return (
    <span
      className={`inline-flex shrink-0 items-center justify-center rounded-[4px] border ${className ?? ""}`}
      style={{ width: size, height: size, backgroundColor: `${color}2e`, borderColor: color }}
    >
      {initials === "?" ? (
        <Icon icon={FALLBACK_GLYPH} width={inner} height={inner} style={{ color }} />
      ) : (
        <span
          className="font-semibold leading-none"
          style={{ color, fontSize: Math.max(7, Math.round(size * 0.45)) }}
        >
          {initials}
        </span>
      )}
    </span>
  );
};

export const AgentIcon = ({ agent, size = 16, className }: { agent: Agent; size?: number; className?: string }) => (
  <BrandIcon name={agentIcon(agent)} fallbackKey={agent} size={size} className={className} />
);

export const ModelIcon = ({ model, size = 16, className }: { model: string | null; size?: number; className?: string }) => (
  <BrandIcon name={modelIcon(model)} fallbackKey={model ?? "?"} size={size} className={className} />
);

export const ProviderIcon = ({
  providerId,
  size = 16,
  className,
}: {
  providerId: string;
  size?: number;
  className?: string;
}) => <BrandIcon name={providerIcon(providerId)} fallbackKey={providerId} size={size} className={className} />;
