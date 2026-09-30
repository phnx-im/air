#include "window_placement.h"

#include <iostream>

namespace {

constexpr wchar_t kRegistryKey[] = L"Software\\Phoenix R&D GmbH\\Air";
constexpr wchar_t kRegistryValue[] = L"WindowPlacement";

}  // namespace

int RestoreWindowPlacement(HWND window) {
  WINDOWPLACEMENT placement = {};
  DWORD size = sizeof(placement);
  LSTATUS status =
      RegGetValueW(HKEY_CURRENT_USER, kRegistryKey, kRegistryValue,
                   RRF_RT_REG_BINARY, nullptr, &placement, &size);
  if (status == ERROR_FILE_NOT_FOUND) {
    return SW_SHOWNORMAL;
  }
  if (status != ERROR_SUCCESS || size != sizeof(placement) ||
      placement.length != sizeof(placement)) {
    std::cerr << "Ignoring saved window placement, status " << status
              << std::endl;
    return SW_SHOWNORMAL;
  }

  int show_command = placement.showCmd == SW_SHOWMAXIMIZED ? SW_SHOWMAXIMIZED
                                                           : SW_SHOWNORMAL;
  // The window stays hidden until Flutter renders its first frame.
  placement.showCmd = SW_HIDE;
  placement.flags = 0;
  // Moving the window to a monitor with a different DPI triggers
  // WM_DPICHANGED, which rescales it. Applying the placement a second time on
  // the new monitor restores the saved size.
  SetWindowPlacement(window, &placement);
  SetWindowPlacement(window, &placement);
  return show_command;
}

void SaveWindowPlacement(HWND window) {
  WINDOWPLACEMENT placement = {};
  placement.length = sizeof(placement);
  if (!GetWindowPlacement(window, &placement)) {
    std::cerr << "Failed to get window placement, error " << GetLastError()
              << std::endl;
    return;
  }
  if (placement.showCmd == SW_SHOWMINIMIZED) {
    placement.showCmd = (placement.flags & WPF_RESTORETOMAXIMIZED)
                            ? SW_SHOWMAXIMIZED
                            : SW_SHOWNORMAL;
  }

  LSTATUS status =
      RegSetKeyValueW(HKEY_CURRENT_USER, kRegistryKey, kRegistryValue,
                      REG_BINARY, &placement, sizeof(placement));
  if (status != ERROR_SUCCESS) {
    std::cerr << "Failed to save window placement, status " << status
              << std::endl;
  }
}
