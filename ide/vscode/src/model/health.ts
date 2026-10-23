// Trunk health as a status bar item.

import type { Health } from "../cli/schema";

export interface HealthView {
  text: string;
  tooltip: string;
  alert: boolean;
}

export function healthView(h: Health): HealthView {
  if (h.frozen) {
    return {
      text: "$(lock) trunk frozen",
      tooltip: `Trunk is frozen${h.frozen_reason ? `: ${h.frozen_reason}` : ""}. Landings wait until it thaws.`,
      alert: true,
    };
  }
  if (h.state === "green") {
    return { text: "$(pass) trunk", tooltip: "Trunk is green.", alert: false };
  }
  const steps = h.failing_steps.length ? `\nFailing: ${h.failing_steps.join(", ")}` : "";
  const incidents = h.open.length ? `\nOpen incidents: ${h.open.length}` : "";
  return {
    text: `$(error) trunk ${h.state}`,
    tooltip: `Trunk is ${h.state}.${steps}${incidents}`,
    alert: true,
  };
}
