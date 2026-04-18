import { axe } from "vitest-axe";
import { expect } from "vitest";

export async function expectNoA11yViolations(
  container: Element,
  label = "container",
): Promise<void> {
  const results = await axe(container);
  if (results.violations.length > 0) {
    const details = results.violations
      .map((v) => `- ${v.id}: ${v.help} (${v.nodes.length} node${v.nodes.length === 1 ? "" : "s"})`)
      .join("\n");
    throw new Error(`Accessibility violations in ${label}:\n${details}`);
  }
  expect(results.violations).toHaveLength(0);
}
