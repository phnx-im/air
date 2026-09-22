#include "flutter_window.h"

#include <flutter_windows.h>

#include <optional>

#include "flutter/generated_plugin_registrant.h"
#include "window_placement.h"

namespace {

// Minimum size of the Flutter view in logical pixels.
constexpr int kMinContentWidth = 768;
constexpr int kMinContentHeight = 512;

// Windows expects the minimum size in physical pixels and including the
// window frame, so we scale it for the current monitor and add the frame.
void SetMinTrackSize(HWND window, MINMAXINFO* info) {
  UINT dpi = FlutterDesktopGetDpiForMonitor(
      MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST));
  RECT frame = {0, 0, MulDiv(kMinContentWidth, dpi, 96),
                MulDiv(kMinContentHeight, dpi, 96)};
  AdjustWindowRectExForDpi(
      &frame, static_cast<DWORD>(GetWindowLongPtr(window, GWL_STYLE)), FALSE,
      static_cast<DWORD>(GetWindowLongPtr(window, GWL_EXSTYLE)), dpi);
  info->ptMinTrackSize.x = frame.right - frame.left;
  info->ptMinTrackSize.y = frame.bottom - frame.top;
}

}  // namespace

FlutterWindow::FlutterWindow(const flutter::DartProject& project)
    : project_(project) {}

FlutterWindow::~FlutterWindow() {}

bool FlutterWindow::OnCreate() {
  if (!Win32Window::OnCreate()) {
    return false;
  }

  show_command_ = RestoreWindowPlacement(GetHandle());
  RECT frame = GetClientArea();

  // The size here must match the window dimensions to avoid unnecessary surface
  // creation / destruction in the startup path.
  flutter_controller_ = std::make_unique<flutter::FlutterViewController>(
      frame.right - frame.left, frame.bottom - frame.top, project_);
  // Ensure that basic setup of the controller was successful.
  if (!flutter_controller_->engine() || !flutter_controller_->view()) {
    return false;
  }
  RegisterPlugins(flutter_controller_->engine());
  SetChildContent(flutter_controller_->view()->GetNativeWindow());

  flutter_controller_->engine()->SetNextFrameCallback([&]() {
    ShowWindow(GetHandle(), show_command_);
  });

  // Flutter can complete the first frame before the "show window" callback is
  // registered. The following call ensures a frame is pending to ensure the
  // window is shown. It is a no-op if the first frame hasn't completed yet.
  flutter_controller_->ForceRedraw();

  return true;
}

void FlutterWindow::OnDestroy() {
  if (flutter_controller_) {
    flutter_controller_ = nullptr;
  }

  Win32Window::OnDestroy();
}

LRESULT
FlutterWindow::MessageHandler(HWND hwnd, UINT const message,
                              WPARAM const wparam,
                              LPARAM const lparam) noexcept {
  if (message == WM_CLOSE || (message == WM_ENDSESSION && wparam)) {
    SaveWindowPlacement(hwnd);
  }

  // Give Flutter, including plugins, an opportunity to handle window messages.
  if (flutter_controller_) {
    std::optional<LRESULT> result =
        flutter_controller_->HandleTopLevelWindowProc(hwnd, message, wparam,
                                                      lparam);
    if (result) {
      return *result;
    }
  }

  switch (message) {
    case WM_FONTCHANGE:
      flutter_controller_->engine()->ReloadSystemFonts();
      break;
    case WM_GETMINMAXINFO:
      SetMinTrackSize(hwnd, reinterpret_cast<MINMAXINFO*>(lparam));
      return 0;
  }

  return Win32Window::MessageHandler(hwnd, message, wparam, lparam);
}
