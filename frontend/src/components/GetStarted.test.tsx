import { describe, it, expect, vi, beforeEach } from "vitest"
import { render, screen, fireEvent, waitFor } from "@testing-library/react"
import { BrowserRouter } from "react-router-dom"
import { GetStarted } from "./GetStarted"

const mockNavigate = vi.fn()

vi.mock("react-router-dom", async () => {
  const actual = await vi.importActual("react-router-dom")
  return {
    ...actual,
    useNavigate: () => mockNavigate,
  }
})

function renderGetStarted() {
  return render(
    <BrowserRouter>
      <GetStarted />
    </BrowserRouter>
  )
}

describe("GetStarted", () => {
  beforeEach(() => {
    vi.clearAllMocks()
    global.fetch = vi.fn()
  })

  it("renders input field and button", () => {
    renderGetStarted()

    expect(screen.getByLabelText("Project path")).toBeInTheDocument()
    expect(
      screen.getByRole("button", { name: /open project/i })
    ).toBeInTheDocument()
  })

  it("renders welcome text", () => {
    renderGetStarted()

    expect(screen.getByText("Welcome to Plasma")).toBeInTheDocument()
    expect(
      screen.getByText(/enter the path to your .+ project to get started/i)
    ).toBeInTheDocument()
  })

  it("disables button when input is empty", () => {
    renderGetStarted()

    const button = screen.getByRole("button", { name: /open project/i })
    expect(button).toBeDisabled()
  })

  it("enables button when input has value", () => {
    renderGetStarted()

    const input = screen.getByLabelText("Project path")
    fireEvent.change(input, { target: { value: "/path/to/project" } })

    const button = screen.getByRole("button", { name: /open project/i })
    expect(button).not.toBeDisabled()
  })

  it("shows error when validation fails", async () => {
    vi.mocked(global.fetch).mockResolvedValueOnce({
      json: () =>
        Promise.resolve({
          valid: "false",
          error: "No Xcode or Android project found",
        }),
    } as Response)

    renderGetStarted()

    const input = screen.getByLabelText("Project path")
    fireEvent.change(input, { target: { value: "/invalid/path" } })

    const button = screen.getByRole("button", { name: /open project/i })
    fireEvent.click(button)

    await waitFor(() => {
      expect(
        screen.getByText("No Xcode or Android project found")
      ).toBeInTheDocument()
    })

    expect(mockNavigate).not.toHaveBeenCalled()
  })

  it("navigates on successful validation", async () => {
    vi.mocked(global.fetch).mockResolvedValueOnce({
      json: () =>
        Promise.resolve({
          valid: "true",
          type: "xcode",
          name: "MyApp",
        }),
    } as Response)

    renderGetStarted()

    const input = screen.getByLabelText("Project path")
    fireEvent.change(input, { target: { value: "/path/to/MyApp" } })

    const button = screen.getByRole("button", { name: /open project/i })
    fireEvent.click(button)

    await waitFor(() => {
      expect(mockNavigate).toHaveBeenCalledWith(
        "/?project=%2Fpath%2Fto%2FMyApp"
      )
    })
  })

  it("shows loading state during validation", async () => {
    vi.mocked(global.fetch).mockImplementationOnce(
      () =>
        new Promise((resolve) =>
          setTimeout(
            () =>
              resolve({
                json: () => Promise.resolve({ valid: "true", type: "xcode", name: "MyApp" }),
              } as Response),
            100
          )
        )
    )

    renderGetStarted()

    const input = screen.getByLabelText("Project path")
    fireEvent.change(input, { target: { value: "/path/to/project" } })

    const button = screen.getByRole("button", { name: /open project/i })
    fireEvent.click(button)

    expect(screen.getByText("Validating...")).toBeInTheDocument()
  })

  it("shows error when fetch fails", async () => {
    vi.mocked(global.fetch).mockRejectedValueOnce(new Error("Network error"))

    renderGetStarted()

    const input = screen.getByLabelText("Project path")
    fireEvent.change(input, { target: { value: "/path/to/project" } })

    const button = screen.getByRole("button", { name: /open project/i })
    fireEvent.click(button)

    await waitFor(() => {
      expect(
        screen.getByText("Failed to validate project. Please try again.")
      ).toBeInTheDocument()
    })
  })
})
