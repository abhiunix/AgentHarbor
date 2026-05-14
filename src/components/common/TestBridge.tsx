/**
 * TestBridge — Development/test component for fast GUI testing.
 *
 * Polls for test commands via Tauri IPC (file-based bridge) and responds
 * with DOM element coordinates. Eliminates the need for vision models.
 *
 * Protocol:
 *   Python writes /tmp/agentharbor_test_cmd.json
 *   This component reads it via test_bridge_read_cmd Tauri command
 *   Processes the command (querySelector, getBoundingClientRect)
 *   Writes result via test_bridge_write_result Tauri command
 *   Python reads /tmp/agentharbor_test_result.json
 */

import { useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";

const POLL_INTERVAL = 100; // ms

interface TestCommand {
  id: string;
  action: "query" | "query_all" | "click" | "exists" | "text" | "scroll" | "query_caps" | "query_agents" | "input";
  testid?: string;
  selector?: string;
  text?: string;
  value?: string;
  cap_type?: string; // mcp, rule, skill, hook, plugin
  cap_name?: string; // partial match on capability name
  agent_name?: string; // partial match on agent name
}

interface ElementRect {
  found: boolean;
  x: number;
  y: number;
  width: number;
  height: number;
  centerX: number;
  centerY: number;
  text?: string;
  visible?: boolean;
}

function queryElement(testid?: string, selector?: string): ElementRect {
  const sel = testid ? `[data-testid="${testid}"]` : selector || "";
  const el = document.querySelector(sel) as HTMLElement | null;
  if (!el) return { found: false, x: 0, y: 0, width: 0, height: 0, centerX: 0, centerY: 0 };

  const r = el.getBoundingClientRect();
  const visible = r.width > 0 && r.height > 0 &&
    window.getComputedStyle(el).display !== "none" &&
    window.getComputedStyle(el).visibility !== "hidden";

  return {
    found: true,
    x: r.x,
    y: r.y,
    width: r.width,
    height: r.height,
    centerX: r.x + r.width / 2,
    centerY: r.y + r.height / 2,
    text: el.textContent?.trim().slice(0, 200),
    visible,
  };
}

function queryAllElements(selector: string): ElementRect[] {
  const els = document.querySelectorAll(selector);
  return Array.from(els).map((el) => {
    const r = el.getBoundingClientRect();
    return {
      found: true,
      x: r.x,
      y: r.y,
      width: r.width,
      height: r.height,
      centerX: r.x + r.width / 2,
      centerY: r.y + r.height / 2,
      text: el.textContent?.trim().slice(0, 200),
      visible: r.width > 0 && r.height > 0,
    };
  });
}

function findByText(text: string): ElementRect {
  const selectors = "button, a, [role='button'], [data-testid], input, select, [onclick], div[class*='cursor-pointer']";
  const els = document.querySelectorAll(selectors);
  for (const el of els) {
    if (el.textContent?.trim().includes(text)) {
      const r = el.getBoundingClientRect();
      if (r.width > 0 && r.height > 0) {
        return {
          found: true,
          x: r.x, y: r.y, width: r.width, height: r.height,
          centerX: r.x + r.width / 2, centerY: r.y + r.height / 2,
          text: el.textContent?.trim().slice(0, 200),
          visible: true,
        };
      }
    }
  }
  // Fallback: search all elements for direct text content
  const allEls = document.querySelectorAll("*");
  for (const el of allEls) {
    const directText = Array.from(el.childNodes)
      .filter(n => n.nodeType === Node.TEXT_NODE)
      .map(n => n.textContent?.trim())
      .join(" ");
    if (directText.includes(text)) {
      const r = (el as HTMLElement).getBoundingClientRect();
      if (r.width > 0 && r.height > 0) {
        return {
          found: true,
          x: r.x, y: r.y, width: r.width, height: r.height,
          centerX: r.x + r.width / 2, centerY: r.y + r.height / 2,
          text: directText.slice(0, 200),
          visible: true,
        };
      }
    }
  }
  return { found: false, x: 0, y: 0, width: 0, height: 0, centerX: 0, centerY: 0 };
}

function findElement(testid?: string, selector?: string, text?: string): HTMLElement | null {
  if (testid) {
    return document.querySelector(`[data-testid="${testid}"]`) as HTMLElement | null;
  }
  if (selector) {
    return document.querySelector(selector) as HTMLElement | null;
  }
  if (text) {
    const selectors = "button, a, [role='button'], [data-testid], input, textarea, select, [onclick], div[class*='cursor-pointer']";
    const els = document.querySelectorAll(selectors);
    for (const el of els) {
      if (el.textContent?.trim().includes(text)) {
        return el as HTMLElement;
      }
    }
  }
  return null;
}

function clickElement(testid?: string, selector?: string, text?: string) {
  const el = findElement(testid, selector, text);
  if (!el) return { clicked: false };
  el.scrollIntoView({ behavior: "instant", block: "center" });
  el.click();
  return { clicked: true };
}

function inputValue(testid?: string, selector?: string, value?: string) {
  const el = findElement(testid, selector);
  if (!el) return { updated: false };
  if (!(el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement || el instanceof HTMLSelectElement)) {
    return { updated: false, error: "Element is not an input, textarea, or select" };
  }

  const nextValue = value ?? "";
  const prototype =
    el instanceof HTMLTextAreaElement
      ? HTMLTextAreaElement.prototype
      : el instanceof HTMLSelectElement
        ? HTMLSelectElement.prototype
        : HTMLInputElement.prototype;
  const setter = Object.getOwnPropertyDescriptor(prototype, "value")?.set;
  if (setter) {
    setter.call(el, nextValue);
  } else {
    el.value = nextValue;
  }

  el.dispatchEvent(new Event("input", { bubbles: true }));
  el.dispatchEvent(new Event("change", { bubbles: true }));
  return { updated: true, value: nextValue };
}

function queryCaps(capType?: string, capName?: string): Array<ElementRect & { capType: string; capName: string; testid: string }> {
  const rows = document.querySelectorAll('[data-testid^="deploy-cap-"]');
  const results: Array<ElementRect & { capType: string; capName: string; testid: string }> = [];
  for (const el of rows) {
    const htmlEl = el as HTMLElement;
    const type = htmlEl.getAttribute("data-cap-type") || "";
    const name = htmlEl.getAttribute("data-cap-name") || "";
    const tid = htmlEl.getAttribute("data-testid") || "";

    // Filter by type if specified
    if (capType && type !== capType) continue;
    // Filter by name (partial match) if specified
    if (capName && !name.toLowerCase().includes(capName.toLowerCase())) continue;

    const r = htmlEl.getBoundingClientRect();
    results.push({
      found: true,
      x: r.x, y: r.y, width: r.width, height: r.height,
      centerX: r.x + r.width / 2, centerY: r.y + r.height / 2,
      text: name,
      visible: r.width > 0 && r.height > 0,
      capType: type,
      capName: name,
      testid: tid,
    });
  }
  return results;
}

function queryAgents(agentName?: string): Array<ElementRect & { agentName: string; agentModel: string; testid: string }> {
  const rows = document.querySelectorAll('[data-testid^="deploy-agent-"]');
  const results: Array<ElementRect & { agentName: string; agentModel: string; testid: string }> = [];
  for (const el of rows) {
    const htmlEl = el as HTMLElement;
    const name = htmlEl.getAttribute("data-agent-name") || "";
    const model = htmlEl.getAttribute("data-agent-model") || "";
    const tid = htmlEl.getAttribute("data-testid") || "";

    if (agentName && !name.toLowerCase().includes(agentName.toLowerCase())) continue;

    const r = htmlEl.getBoundingClientRect();
    results.push({
      found: true,
      x: r.x, y: r.y, width: r.width, height: r.height,
      centerX: r.x + r.width / 2, centerY: r.y + r.height / 2,
      text: name,
      visible: r.width > 0 && r.height > 0,
      agentName: name,
      agentModel: model,
      testid: tid,
    });
  }
  return results;
}

function handleCommand(cmd: TestCommand): unknown {
  switch (cmd.action) {
    case "query":
      return queryElement(cmd.testid, cmd.selector);
    case "query_all":
      return queryAllElements(cmd.selector || `[data-testid]`);
    case "exists":
      return { exists: !!document.querySelector(cmd.testid ? `[data-testid="${cmd.testid}"]` : cmd.selector || "") };
    case "click":
      return clickElement(cmd.testid, cmd.selector, cmd.text);
    case "text":
      return findByText(cmd.text || "");
    case "input":
      return inputValue(cmd.testid, cmd.selector, cmd.value);
    case "scroll": {
      const el = cmd.testid ? document.querySelector(`[data-testid="${cmd.testid}"]`) as HTMLElement : null;
      if (el) el.scrollIntoView({ behavior: "smooth", block: "center" });
      return { scrolled: !!el };
    }
    case "query_caps":
      return queryCaps(cmd.cap_type, cmd.cap_name);
    case "query_agents":
      return queryAgents(cmd.agent_name);
    default:
      return { error: `Unknown action: ${cmd.action}` };
  }
}

export function TestBridge() {
  const timerRef = useRef<ReturnType<typeof setInterval>>(undefined);

  useEffect(() => {
    const poll = async () => {
      try {
        const raw = await invoke<string>("test_bridge_read_cmd");
        if (!raw) return; // No pending command

        const cmd: TestCommand = JSON.parse(raw);
        const result = handleCommand(cmd);
        const response = JSON.stringify({ id: cmd.id, result });
        await invoke("test_bridge_write_result", { data: response });
      } catch {
        // Command not available or parse error — silently ignore
      }
    };

    timerRef.current = setInterval(poll, POLL_INTERVAL);
    return () => {
      if (timerRef.current) clearInterval(timerRef.current);
    };
  }, []);

  return null;
}
