import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Field } from "@/shared/ui/Field";
import { Input } from "@/shared/ui/Input";

describe("Field", () => {
  it("wires the label and the hint to the control", () => {
    render(
      <Field id="service-name" label="Name" hint="Shown on the card">
        <Input id="service-name" />
      </Field>,
    );
    const control = screen.getByLabelText("Name");
    expect(control).toBeInTheDocument();
    expect(control).toHaveAccessibleDescription("Shown on the card");
  });

  it("works without a hint", () => {
    render(
      <Field id="service-name" label="Name">
        <Input id="service-name" />
      </Field>,
    );
    expect(screen.getByLabelText("Name")).toBeInTheDocument();
  });

  it("shows an error in words and associates it with the control", () => {
    render(
      <Field
        id="service-name"
        label="Name"
        hint="Shown on the card"
        error="Enter a name"
      >
        <Input id="service-name" invalid />
      </Field>,
    );
    const control = screen.getByLabelText("Name");
    expect(screen.getByText("Enter a name")).toBeVisible();
    expect(control).toHaveAccessibleDescription(
      "Shown on the card Enter a name",
    );
  });
});
