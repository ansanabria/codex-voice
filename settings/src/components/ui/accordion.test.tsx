import { fireEvent, render, screen } from "@testing-library/react"
import "@testing-library/jest-dom/vitest"
import { expect, test, vi } from "vitest"

import { Accordion, AccordionContent, AccordionItem, AccordionTrigger } from "./accordion"

test("expanded content stays in normal flow and remains interactive", () => {
  const onReset = vi.fn()

  render(
    <Accordion type="single" collapsible>
      <AccordionItem value="advanced">
        <AccordionTrigger>Advanced</AccordionTrigger>
        <AccordionContent>
          <button onClick={onReset}>Reset all settings</button>
        </AccordionContent>
      </AccordionItem>
    </Accordion>,
  )

  fireEvent.click(screen.getByRole("button", { name: "Advanced" }))

  const reset = screen.getByRole("button", { name: "Reset all settings" })
  expect(reset.parentElement).not.toHaveClass("h-(--radix-accordion-content-height)")
  expect(reset).toBeVisible()
  fireEvent.click(reset)
  expect(onReset).toHaveBeenCalledOnce()
})
