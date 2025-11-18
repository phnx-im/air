// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';
import 'package:air/ui/colors/themes.dart';

/// Left panel which can be resized by dragging the handle
class ResizablePanel extends StatefulWidget {
  const ResizablePanel({
    required this.initialWidth,
    this.minWidth = 200,
    this.maxWidth = 600,
    this.resizeHandleWidth = 10,
    this.onResizeEnd,
    required this.child,
    super.key,
  });

  final double initialWidth;
  final double minWidth;
  final double maxWidth;
  final double resizeHandleWidth;

  final Widget child;

  final Function(double)? onResizeEnd;

  @override
  State<ResizablePanel> createState() => _ResizablePanelState();
}

class _ResizablePanelState extends State<ResizablePanel> {
  late double _panelWidth;

  @override
  void initState() {
    super.initState();
    setState(() {
      _panelWidth = widget.initialWidth;
    });
  }

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      width: _panelWidth,
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          SizedBox(
            width: _panelWidth - widget.resizeHandleWidth,
            child: widget.child,
          ),

          // Resizable Handle
          MouseRegion(
            cursor: SystemMouseCursors.resizeColumn,
            child: GestureDetector(
              onHorizontalDragUpdate: onResize,
              onHorizontalDragEnd: (details) {
                if (widget.onResizeEnd case final onResizeEnd?) {
                  onResizeEnd(_panelWidth);
                }
              },
              child: SizedBox(
                width: widget.resizeHandleWidth,
                child: Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Container(
                      width: widget.resizeHandleWidth / 2,
                      decoration: BoxDecoration(
                        shape: BoxShape.rectangle,
                        border: Border(
                          left: BorderSide(
                            width: 1,
                            color: CustomColorScheme.of(
                              context,
                            ).separator.secondary,
                          ),
                        ),
                      ),
                    ),
                  ],
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }

  void onResize(DragUpdateDetails details) {
    setState(() {
      _panelWidth = (_panelWidth + details.delta.dx).clamp(
        widget.minWidth,
        widget.maxWidth,
      );
    });
  }
}
