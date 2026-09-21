import { useState } from "react";
import { Inbox } from "lucide-react";
import { toast } from "sonner";
import { Button } from "../../Button";
import { Card } from "../../Card";
import { EmptyState } from "../../EmptyState";
import { Modal } from "../../Modal";
import { SectionHeader } from "../../SectionHeader";
import { SettingRow } from "../../SettingRow";
import { Toaster } from "../../Toaster";

export function LayoutSection() {
  const [modalOpen, setModalOpen] = useState(false);

  return (
    <section className="flex flex-col gap-4">
      <h2 className="text-title text-content">Layout</h2>

      <SectionHeader
        title="AI Tools"
        description="Install and update the tools you code with"
        action={<Button size="sm">Check</Button>}
      />

      <div className="grid gap-4 md:grid-cols-2">
        <Card>Default card</Card>
        <Card interactive>Hover card</Card>
        <Card padding="sm">Small padding</Card>
        <Card padding="lg">Large padding</Card>
      </div>

      <Card padding="none" className="px-4">
        <SettingRow
          label="Start at login"
          description="Opens when you sign in"
          controlId="gallery-start-at-login"
        >
          <input id="gallery-start-at-login" type="checkbox" />
        </SettingRow>
        <SettingRow label="Version">
          <span className="text-mono-sm font-mono text-content-muted">
            3.19.2
          </span>
        </SettingRow>
      </Card>

      <Card padding="none">
        <EmptyState
          icon={Inbox}
          title="Empty"
          description="Nothing to show here yet"
          action={<Button size="sm">Install</Button>}
        />
      </Card>

      <div className="flex flex-wrap gap-3">
        <Button variant="secondary" onClick={() => setModalOpen(true)}>
          Open modal
        </Button>
        <Button variant="secondary" onClick={() => toast.success("Ready")}>
          Toast success
        </Button>
        <Button
          variant="secondary"
          onClick={() => toast.warning("Needs Attention")}
        >
          Toast warning
        </Button>
        <Button
          variant="secondary"
          onClick={() => toast.error("Action Required")}
        >
          Toast error
        </Button>
      </div>

      <Modal
        open={modalOpen}
        onOpenChange={setModalOpen}
        title="Remove Claude Code"
        description="Your settings stay on this computer."
        footer={
          <>
            <Button variant="ghost" onClick={() => setModalOpen(false)}>
              Cancel
            </Button>
            <Button variant="danger" onClick={() => setModalOpen(false)}>
              Remove
            </Button>
          </>
        }
      >
        This removes the tool from your computer.
      </Modal>

      <Toaster />
    </section>
  );
}
