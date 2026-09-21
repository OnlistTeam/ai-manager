/** @type {import('tailwindcss').Config} */
module.exports = {
  content: ["./src/index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  darkMode: ["selector", ".dark"],
  theme: {
    extend: {
      colors: {
        background: "hsl(var(--background))",
        foreground: "hsl(var(--foreground))",
        card: {
          DEFAULT: "hsl(var(--card))",
          foreground: "hsl(var(--card-foreground))",
        },
        popover: {
          DEFAULT: "hsl(var(--popover))",
          foreground: "hsl(var(--popover-foreground))",
        },
        primary: {
          DEFAULT: "hsl(var(--primary))",
          foreground: "hsl(var(--primary-foreground))",
        },
        secondary: {
          DEFAULT: "hsl(var(--secondary))",
          foreground: "hsl(var(--secondary-foreground))",
        },
        muted: {
          DEFAULT: "hsl(var(--muted))",
          foreground: "hsl(var(--muted-foreground))",
        },
        // `--accent` / `--accent-foreground` are spec §46 product tokens (hex),
        // so the shadcn "subtle hover surface" semantics point at the bridge
        // variables directly instead of at same-named CSS variables.
        accent: {
          DEFAULT: "hsl(var(--ui-surface-hover))",
          foreground: "hsl(var(--ui-text-primary))",
        },
        destructive: {
          DEFAULT: "hsl(var(--destructive))",
          foreground: "hsl(var(--destructive-foreground))",
        },
        // `--border` is a spec §46 product token (hex); the Tailwind key uses
        // the HSL bridge so `border-border/50` keeps working.
        border: "hsl(var(--ui-border))",
        input: "hsl(var(--input))",
        ring: "hsl(var(--ring))",

        // Product vocabulary (spec §46). New components in src/shared/ui speak
        // this dialect; legacy code keeps using the shadcn names above. Both
        // resolve to the same `--ui-*` bridge variables in tokens.css.
        canvas: {
          DEFAULT: "hsl(var(--ui-bg-primary))",
          subtle: "hsl(var(--ui-bg-secondary))",
        },
        surface: {
          DEFAULT: "hsl(var(--ui-surface))",
          hover: "hsl(var(--ui-surface-hover))",
        },
        content: {
          DEFAULT: "hsl(var(--ui-text-primary))",
          muted: "hsl(var(--ui-text-secondary))",
        },
        line: "hsl(var(--ui-border))",

        // Lift layers (tokens.css group 1). Depth on the coloured canvas is
        // made of translucent white film, so these carry their own alpha and
        // must not be combined with Tailwind's `/alpha` modifier.
        layer: {
          1: "var(--layer-1)",
          2: "var(--layer-2)",
          3: "var(--layer-3)",
        },
        hairline: {
          DEFAULT: "var(--hairline)",
          strong: "var(--hairline-strong)",
        },
        brand: {
          DEFAULT: "hsl(var(--ui-accent))",
          hover: "hsl(var(--ui-accent-hover))",
          foreground: "hsl(var(--ui-accent-foreground))",
        },
        // Status colours never carry text: contrast on the light canvas is
        // 3.29 / 2.81 / 4.23. They are only used on icons, borders and 10%
        // tints; labels stay on `text-content` (spec §98).
        success: "hsl(var(--ui-success))",
        warning: "hsl(var(--ui-warning))",
        danger: "hsl(var(--ui-danger))",
        blue: {
          400: "#409CFF",
          500: "#0A84FF",
          600: "#0060DF",
        },
        gray: {
          50: "#fafafa",
          100: "#f4f4f5",
          200: "#e4e4e7",
          300: "#d4d4d8",
          400: "#a1a1aa",
          500: "#71717a",
          600: "#636366",
          700: "#48484A",
          800: "#3A3A3C",
          900: "#2C2C2E",
          950: "#1C1C1E",
        },
        green: {
          100: "#d1fae5",
          500: "#10b981",
        },
        red: {
          100: "#fee2e2",
          500: "#ef4444",
        },
        amber: {
          100: "#fef3c7",
          500: "#f59e0b",
        },
      },
      boxShadow: {
        sm: "var(--shadow-sm)",
        md: "var(--shadow-md)",
        lg: "var(--shadow-lg)",
      },
      borderRadius: {
        sm: "var(--radius-sm)",
        md: "var(--radius-md)",
        lg: "var(--radius-lg)",
        xl: "var(--radius-xl)",
        "2xl": "var(--radius-2xl)",
      },
      fontFamily: {
        sans: ["var(--font-sans)"],
        mono: ["var(--font-mono)"],
      },
      // Spec §50 type scale. `text-body` (14px) is the baseline; `text-display`
      // is the home greeting, `text-mono-sm` is for versions and log lines.
      fontSize: {
        display: [
          "28px",
          { lineHeight: "34px", letterSpacing: "-0.02em", fontWeight: "600" },
        ],
        title: ["20px", { lineHeight: "26px", fontWeight: "600" }],
        heading: ["16px", { lineHeight: "22px", fontWeight: "600" }],
        body: ["14px", { lineHeight: "20px", fontWeight: "400" }],
        caption: ["12px", { lineHeight: "16px", fontWeight: "400" }],
        "mono-sm": ["12px", { lineHeight: "16px" }],
      },
      transitionDuration: {
        fast: "var(--motion-fast)",
        base: "var(--motion-base)",
        modal: "var(--motion-modal)",
        page: "var(--motion-page)",
      },
      transitionTimingFunction: {
        standard: "var(--ease-standard)",
        page: "var(--ease-page)",
        spring: "var(--ease-spring)",
      },
      animation: {
        // Durations come from the motion tokens, which the global
        // prefers-reduced-motion media query zeroes out.
        "ds-overlay-in":
          "ds-overlay-in var(--motion-modal) var(--ease-standard)",
        "ds-dialog-in": "ds-dialog-in var(--motion-modal) var(--ease-standard)",
        "ds-modal-in": "ds-modal-in var(--motion-modal) var(--ease-standard)",
        "page-in": "page-in var(--motion-page) var(--ease-page) both",
        "fade-in": "fadeIn 0.5s ease-out",
        "slide-up": "slideUp 0.5s ease-out",
        "slide-down": "slideDown 0.3s ease-out",
        "slide-in-right": "slideInRight 0.3s ease-out",
        "pulse-slow": "pulse 3s cubic-bezier(0.4, 0, 0.6, 1) infinite",
        "accordion-down": "accordion-down 0.2s ease-out",
        "accordion-up": "accordion-up 0.2s ease-out",
      },
      keyframes: {
        "ds-overlay-in": {
          from: { opacity: "0" },
          to: { opacity: "1" },
        },
        "ds-modal-in": {
          from: { opacity: "0", transform: "translateY(8px) scale(0.98)" },
          to: { opacity: "1", transform: "translateY(0) scale(1)" },
        },
        "ds-dialog-in": {
          from: {
            opacity: "0",
            transform: "translate(-50%, calc(-50% + 8px)) scale(0.98)",
          },
          to: {
            opacity: "1",
            transform: "translate(-50%, -50%) scale(1)",
          },
        },
        "page-in": {
          from: {
            opacity: "0",
            transform: "translate3d(0, var(--page-enter-y, 44px), 0)",
          },
          to: { opacity: "1", transform: "translate3d(0, 0, 0)" },
        },
        fadeIn: {
          "0%": {
            opacity: "0",
          },
          "100%": {
            opacity: "1",
          },
        },
        slideUp: {
          "0%": {
            transform: "translateY(20px)",
            opacity: "0",
          },
          "100%": {
            transform: "translateY(0)",
            opacity: "1",
          },
        },
        slideDown: {
          "0%": {
            transform: "translateY(-100%)",
            opacity: "0",
          },
          "100%": {
            transform: "translateY(0)",
            opacity: "1",
          },
        },
        slideInRight: {
          "0%": {
            transform: "translateX(100%)",
            opacity: "0",
          },
          "100%": {
            transform: "translateX(0)",
            opacity: "1",
          },
        },
        "accordion-down": {
          from: {
            height: "0",
          },
          to: {
            height: "var(--radix-accordion-content-height)",
          },
        },
        "accordion-up": {
          from: {
            height: "var(--radix-accordion-content-height)",
          },
          to: {
            height: "0",
          },
        },
      },
    },
  },
  plugins: [],
};
