import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useState,
  type RefObject,
} from "react";
import { stageWires, type StageBox, type StageWire } from "./stageGeometry";

export interface StageWiresState {
  width: number;
  height: number;
  wires: StageWire[];
}

const EMPTY: StageWiresState = { width: 0, height: 0, wires: [] };

function sameWires(left: StageWiresState, right: StageWiresState): boolean {
  return (
    left.width === right.width &&
    left.height === right.height &&
    left.wires.length === right.wires.length &&
    left.wires.every(
      (wire, index) =>
        wire.id === right.wires[index]?.id && wire.d === right.wires[index]?.d,
    )
  );
}

/**
 * Measures the stage's nodes and lays the wires between them. Nodes mark
 * themselves with `data-stage-hub`, `data-stage-source="<wire>"` and
 * `data-stage-target="<wire>"`. All boxes are read in one pass after each
 * commit and whenever the stage resizes; state only changes when a wire
 * actually moved.
 */
export function useStageWires(
  stageRef: RefObject<HTMLElement | null>,
): StageWiresState {
  const [state, setState] = useState<StageWiresState>(EMPTY);

  const measure = useCallback(() => {
    const stage = stageRef.current;
    const hub = stage?.querySelector("[data-stage-hub]");
    if (!stage || !hub) return;
    const origin = stage.getBoundingClientRect();
    const box = (element: Element): StageBox => {
      const rect = element.getBoundingClientRect();
      return {
        left: rect.left - origin.left,
        top: rect.top - origin.top,
        right: rect.right - origin.left,
        bottom: rect.bottom - origin.top,
      };
    };
    const nodes = (attribute: "stageSource" | "stageTarget") =>
      [
        ...stage.querySelectorAll<HTMLElement>(
          attribute === "stageSource"
            ? "[data-stage-source]"
            : "[data-stage-target]",
        ),
      ].map((element) => ({
        wire: element.dataset[attribute] ?? "",
        box: box(element),
      }));
    const next: StageWiresState = {
      width: origin.width,
      height: origin.height,
      wires: stageWires({
        hub: box(hub),
        sources: nodes("stageSource"),
        targets: nodes("stageTarget"),
      }),
    };
    setState((current) => (sameWires(current, next) ? current : next));
  }, [stageRef]);

  // Nodes change size with names, marks and the compact layout; one read
  // per commit keeps the wires on them without tracking each cause.
  useLayoutEffect(() => {
    measure();
  });

  useEffect(() => {
    const stage = stageRef.current;
    if (!stage) return;
    const observer = new ResizeObserver(() => measure());
    observer.observe(stage);
    return () => observer.disconnect();
  }, [measure, stageRef]);

  return state;
}
