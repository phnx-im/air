// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/widgets.dart';
import 'package:air/ds/foundations/foundations.dart';

/// Distance from the panel's edge to the hairline marking the handle. Matches
/// the inset the panel itself sits at, so the handle reads as sitting in the
/// gutter beside the panel instead of on it.
const resizeHandleInset = S.s8;

/// Width of the drag target. Twice the inset, so the hairline it reveals sits
/// in the middle of what the pointer has to hit.
const _handleHitWidth = resizeHandleInset * 2;

/// Length of the hairline that marks the handle while it's engaged.
const _handleLength = S.s48;

/// Two-pane row: dragging a handle on the boundary between the two resizes the
/// leading panel.
///
/// The handle hangs off the content side of that boundary: the panel clips its
/// own surface to round its corners, which would cut off a handle drawn inside
/// it. Overlaying the content instead of taking a column of its own keeps the
/// content centered in what's left of the row.
class ResizablePanel extends StatefulWidget {
  const ResizablePanel({
    required this.initialWidth,
    this.minWidth = 200,
    this.maxWidth = 600,
    this.onResizeEnd,
    required this.panelBuilder,
    required this.content,
    super.key,
  });

  final double initialWidth;
  final double minWidth;
  final double maxWidth;

  /// Builds the panel at the current width. A builder rather than a child, so
  /// the width reaches a pane nested inside whatever group the caller wraps it
  /// in and not just the outermost widget.
  final Widget Function(BuildContext context, double width) panelBuilder;

  /// Fills the rest of the row, with the handle overlaid on its leading edge.
  final Widget content;

  final Function(double)? onResizeEnd;

  @override
  State<ResizablePanel> createState() => _ResizablePanelState();
}

class _ResizablePanelState extends State<ResizablePanel> {
  late double _panelWidth;
  bool _hovered = false;
  bool _resizing = false;

  /// Pointer position and panel width when the current drag started. Resizing
  /// off the total offset keeps the handle under the pointer: accumulating
  /// per-frame deltas instead lets the two drift apart once a drag runs into
  /// the width bounds.
  double _dragStartX = 0;
  double _dragStartWidth = 0;

  @override
  void initState() {
    super.initState();
    _panelWidth = widget.initialWidth;
  }

  @override
  Widget build(BuildContext context) {
    return Row(
      children: [
        widget.panelBuilder(context, _panelWidth),
        Expanded(
          child: Stack(
            children: [
              Positioned.fill(child: widget.content),
              Positioned(
                top: 0,
                bottom: 0,
                left: 0,
                width: _handleHitWidth,
                child: MouseRegion(
                  cursor: SystemMouseCursors.resizeColumn,
                  onEnter: (_) => setState(() => _hovered = true),
                  onExit: (_) => setState(() => _hovered = false),
                  child: GestureDetector(
                    behavior: HitTestBehavior.opaque,
                    onHorizontalDragStart: (details) {
                      _dragStartX = details.globalPosition.dx;
                      _dragStartWidth = _panelWidth;
                      setState(() => _resizing = true);
                    },
                    onHorizontalDragUpdate: onResize,
                    onHorizontalDragEnd: (details) {
                      setState(() => _resizing = false);
                      if (widget.onResizeEnd case final onResizeEnd?) {
                        onResizeEnd(_panelWidth);
                      }
                    },
                    child: Center(
                      // A drag keeps the handle lit even once the pointer
                      // wanders off the narrow target, and hover alone reveals
                      // it.
                      child: AnimatedOpacity(
                        opacity: _hovered || _resizing ? 1 : 0,
                        duration: Effect.duration(MotionPreset.instant),
                        curve: Effect.easeOutQuart,
                        child: SizedBox(
                          width: StrokeWidth.px1,
                          height: _handleLength,
                          child: ColoredBox(
                            color: SemanticPalette.of(
                              context,
                            ).separator.primary,
                          ),
                        ),
                      ),
                    ),
                  ),
                ),
              ),
            ],
          ),
        ),
      ],
    );
  }

  void onResize(DragUpdateDetails details) {
    setState(() {
      _panelWidth = (_dragStartWidth + details.globalPosition.dx - _dragStartX)
          .clamp(widget.minWidth, widget.maxWidth);
    });
  }
}
