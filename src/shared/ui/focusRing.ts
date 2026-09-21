/**
 * The one focus treatment every interactive DS component uses (spec §98:
 * every control must be keyboard operable and visibly focusable).
 *
 * Two concentric rings, not one: `index.css`'s `*:focus-visible` already
 * paints an outline just outside every focused control. That alone
 * disappears on a `bg-brand` fill (a selected ScopeTabs pill, a primary
 * Button) because the ring and the fill are the same colour. This adds the
 * missing half — an inner ring in `brand-foreground`, the token this design
 * system already uses for "readable on top of brand fill".
 *
 * Both rings are deliberately quiet: a single low-opacity hairline each.
 * Focus has to be unmistakable to someone tabbing through, without turning
 * every clicked field into a lit-up halo.
 */
export const FOCUS_RING =
  "focus-visible:ring-1 focus-visible:ring-brand-foreground/40 focus-visible:ring-offset-0";
