#ifndef RUNNER_WINDOW_PLACEMENT_H_
#define RUNNER_WINDOW_PLACEMENT_H_

#include <windows.h>

// Applies the placement saved by |SaveWindowPlacement| to the hidden |window|.
// Returns the show command to use when the window is first shown.
int RestoreWindowPlacement(HWND window);

// Saves the size, position and maximized state of |window| for the next start.
void SaveWindowPlacement(HWND window);

#endif  // RUNNER_WINDOW_PLACEMENT_H_
