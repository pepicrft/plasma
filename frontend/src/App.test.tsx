import { describe, it, expect } from "vitest"
import { render, screen } from "@testing-library/react"
import { MemoryRouter } from "react-router-dom"
import App from "./App"

function renderApp(initialEntries: string[] = ["/"]) {
  return render(
    <MemoryRouter initialEntries={initialEntries}>
      <App />
    </MemoryRouter>
  )
}

describe("App", () => {
  it("shows GetStarted when no project param", () => {
    renderApp(["/"])

    expect(screen.getByText("Welcome to Plasma")).toBeInTheDocument()
    expect(screen.getByLabelText("Project path")).toBeInTheDocument()
  })

  it("shows MainLayout when project param exists", () => {
    renderApp(["/?project=/path/to/MyProject"])

    expect(screen.getByText("MyProject")).toBeInTheDocument()
    expect(screen.getByText("Editor")).toBeInTheDocument()
    expect(screen.getByText("AI Assistant")).toBeInTheDocument()
    expect(screen.getByText("Preview")).toBeInTheDocument()
  })

  it("displays project name in header from path", () => {
    renderApp(["/?project=/Users/dev/projects/AwesomeApp"])

    expect(screen.getByText("AwesomeApp")).toBeInTheDocument()
  })
})
