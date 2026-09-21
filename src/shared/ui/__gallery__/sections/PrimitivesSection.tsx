import * as React from "react";
import { CheckCircle2, RefreshCw } from "lucide-react";
import { Badge } from "../../Badge";
import { Button } from "../../Button";
import { Progress } from "../../Progress";
import { StatusBadge } from "../../StatusBadge";

function Row({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex flex-wrap items-center gap-3">
      <span className="w-24 shrink-0 text-caption text-content-muted">
        {label}
      </span>
      {children}
    </div>
  );
}

export function PrimitivesSection() {
  return (
    <section className="flex flex-col gap-4">
      <h2 className="text-title text-content">Primitives</h2>

      <Row label="Default">
        <Button>Install</Button>
        <Button variant="secondary">Open</Button>
        <Button variant="ghost">Remove</Button>
        <Button variant="danger">Remove</Button>
      </Row>
      <Row label="Loading">
        <Button loading>Install</Button>
        <Button variant="secondary" loading>
          Open
        </Button>
      </Row>
      <Row label="Icon loading">
        <Button data-gallery-button="idle-icon" variant="secondary">
          <RefreshCw className="h-4 w-4" aria-hidden="true" />
          Refresh
        </Button>
        <Button data-gallery-button="loading-icon" variant="secondary" loading>
          <RefreshCw className="h-4 w-4" aria-hidden="true" />
          Refresh
        </Button>
      </Row>
      <Row label="Disabled">
        <Button disabled>Install</Button>
        <Button variant="secondary" disabled>
          Open
        </Button>
      </Row>
      <Row label="Hover">
        <Button className="bg-brand-hover">Install</Button>
        <Button variant="secondary" className="bg-layer-2">
          Open
        </Button>
      </Row>
      <Row label="Focus">
        {/* Static preview of FOCUS_RING's two-tone halo (no real keyboard
            focus in the gallery): a dark ring hugging the control, plus the
            global outline (index.css) standing in for the accent ring
            outside it — see src/shared/ui/focusRing.ts. */}
        <Button className="outline outline-2 outline-brand outline-offset-2 ring-2 ring-brand-foreground ring-offset-0">
          Install
        </Button>
      </Row>
      <Row label="Sizes">
        <Button size="sm">Check</Button>
        <Button size="md">Check</Button>
        <Button size="lg">Check</Button>
      </Row>

      <Row label="Badges">
        <Badge>Neutral</Badge>
        <Badge tone="brand">Brand</Badge>
        <Badge tone="success" icon={CheckCircle2}>
          Installed
        </Badge>
        <Badge tone="warning">Update available</Badge>
        <Badge tone="danger">Needs a fix</Badge>
      </Row>
      <Row label="Status">
        <StatusBadge status="ready" />
        <StatusBadge status="attention" />
        <StatusBadge status="action" />
      </Row>

      <Row label="Progress">
        <div className="w-64">
          <Progress value={0} label="Zero" />
        </div>
        <div className="w-64">
          <Progress value={72} label="Seventy two" />
        </div>
        <div className="w-64">
          <Progress value={100} label="Done" tone="success" />
        </div>
        <div className="w-64">
          <Progress value={0} label="Working" indeterminate />
        </div>
      </Row>
    </section>
  );
}
