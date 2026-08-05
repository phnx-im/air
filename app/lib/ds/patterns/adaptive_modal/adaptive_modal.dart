// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';
import 'dart:math' as math;
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/components/button/button.dart';
import 'package:air/ds/patterns/adaptive_modal/bottom_sheet_tokens.dart';
import 'package:air/ds/patterns/dialog/app_dialog.dart';
import 'package:flutter/material.dart';

/// Displays [builder] on the modal surface the device calls for: a bottom
/// sheet on a phone, a centered dialog card on desktop.
///
/// The choice lives here rather than at the call sites so a sheet -- a touch
/// idiom, reached by thumb and dismissed by drag -- can never reach a pointer.
/// [contentPadding] and [enableDrag] shape the sheet only. The dialog takes its
/// padding from [DialogTokens] and has no handle to drag.
Future<T?> showAdaptiveModal<T>({
  required BuildContext context,
  required WidgetBuilder builder,
  EdgeInsetsGeometry? contentPadding,
  bool isDismissible = true,
  bool enableDrag = true,
  Color? barrierColor,
}) {
  if (DeviceType.isDesktop) {
    return showDialog<T>(
      context: context,
      barrierDismissible: isDismissible,
      barrierColor: barrierColor,
      // The sheet scrolls its content, so the dialog does too rather than
      // overflowing on a short window.
      builder: (dialogContext) => AppDialog(
        child: SingleChildScrollView(child: builder(dialogContext)),
      ),
    );
  }
  return _showBottomSheet<T>(
    context: context,
    builder: builder,
    contentPadding: contentPadding,
    isDismissible: isDismissible,
    enableDrag: enableDrag,
    barrierColor: barrierColor,
  );
}

/// Displays a full-width card anchored to the bottom edge that slides up over
/// a scrim.
///
/// The card hugs its content, growing until it runs out of room below the top
/// safe area. A child that wants a taller surface sizes itself. Tapping the
/// scrim or dragging the handle downward dismisses it.
Future<T?> _showBottomSheet<T>({
  required BuildContext context,
  required WidgetBuilder builder,
  EdgeInsetsGeometry? contentPadding,
  bool isDismissible = true,
  bool enableDrag = true,
  Color? barrierColor,
}) {
  const tokens = BottomSheetTokens.standard;
  return Navigator.of(context, rootNavigator: true).push<T>(
    _BottomSheetRoute<T>(
      barrierDismissible: isDismissible,
      barrierLabel: MaterialLocalizations.of(context).modalBarrierDismissLabel,
      transitionDuration: Effect.duration(tokens.enter),
      reverseDuration: Effect.duration(tokens.exit),
      // The page drives every part of the transition off the route animation,
      // so the route itself hands the child through untouched.
      transitionBuilder: (context, animation, secondaryAnimation, child) =>
          child,
      pageBuilder: (context, animation, secondaryAnimation) =>
          _BottomSheetModal(
            animation: animation,
            builder: builder,
            enableDrag: enableDrag,
            contentPadding: contentPadding,
            scrimColor: barrierColor,
          ),
    ),
  );
}

/// A dialog route whose exit is quicker than its entry, which
/// [RawDialogRoute] alone can't express.
class _BottomSheetRoute<T> extends RawDialogRoute<T> {
  // A transparent barrier because the page paints the scrim itself, on the
  // same animation as the card. The barrier still owns tap-to-dismiss and the
  // dismiss semantics.
  _BottomSheetRoute({
    required super.pageBuilder,
    required super.barrierDismissible,
    required super.barrierLabel,
    required super.transitionDuration,
    required super.transitionBuilder,
    required this.reverseDuration,
  }) : super(barrierColor: Colors.transparent);

  final Duration reverseDuration;

  @override
  Duration get reverseTransitionDuration => reverseDuration;
}

class _BottomSheetModal extends StatefulWidget {
  const _BottomSheetModal({
    required this.animation,
    required this.builder,
    required this.enableDrag,
    this.contentPadding,
    this.scrimColor,
  });

  final Animation<double> animation;
  final WidgetBuilder builder;
  final bool enableDrag;
  final EdgeInsetsGeometry? contentPadding;
  final Color? scrimColor;

  @override
  State<_BottomSheetModal> createState() => _BottomSheetModalState();
}

class _BottomSheetModalState extends State<_BottomSheetModal>
    with SingleTickerProviderStateMixin {
  static const _tokens = BottomSheetTokens.standard;
  static final _sheetBorderRadius = BorderRadius.vertical(
    top: Radius.circular(_tokens.topRadius),
  );

  late final CurvedAnimation _appearAnimation;

  late final AnimationController _dragResetController;
  Animation<double>? _dragResetAnimation;
  VoidCallback? _dragResetListener;

  double _dragOffset = 0;

  @override
  void initState() {
    super.initState();

    _appearAnimation = CurvedAnimation(
      parent: widget.animation,
      curve: Effect.easeOutQuart,
    );

    _dragResetController = AnimationController(
      vsync: this,
      duration: Effect.duration(_tokens.dragSnapBack),
    );
  }

  @override
  void dispose() {
    _detachDragResetListener();
    _dragResetController.dispose();
    _appearAnimation.dispose();
    super.dispose();
  }

  void _detachDragResetListener() {
    if (_dragResetListener != null && _dragResetAnimation != null) {
      _dragResetAnimation!.removeListener(_dragResetListener!);
    }
    _dragResetListener = null;
    _dragResetAnimation = null;
  }

  void _animateDragReset() {
    _dragResetController.stop();
    _detachDragResetListener();

    _dragResetAnimation = Tween<double>(begin: _dragOffset, end: 0).animate(
      CurvedAnimation(parent: _dragResetController, curve: Effect.easeOutQuart),
    );

    _dragResetListener = () {
      setState(() {
        _dragOffset = _dragResetAnimation!.value;
      });
    };

    _dragResetAnimation!.addListener(_dragResetListener!);
    _dragResetController
      ..reset()
      ..forward();
  }

  void _handleVerticalDragUpdate(DragUpdateDetails details) {
    if (!widget.enableDrag) return;

    _dragResetController.stop();
    _detachDragResetListener();

    final updated = (_dragOffset + details.delta.dy).clamp(
      0.0,
      double.infinity,
    );
    if (updated != _dragOffset) {
      setState(() {
        _dragOffset = updated;
      });
    }
  }

  void _handleVerticalDragEnd(DragEndDetails details) {
    if (!widget.enableDrag) return;

    final velocity = details.primaryVelocity ?? 0;
    if (velocity > _tokens.dragDismissVelocity ||
        _dragOffset > _tokens.dragDismissDistance) {
      Navigator.of(context).maybePop();
    } else {
      _animateDragReset();
    }
  }

  void _handleVerticalDragCancel() {
    if (!widget.enableDrag) return;
    _animateDragReset();
  }

  @override
  Widget build(BuildContext context) {
    final mediaQuery = MediaQuery.of(context);
    final palette = SemanticPalette.of(context);

    return Stack(
      fit: .expand,
      children: [
        // Transparent to taps so the route's barrier below keeps handling
        // dismissal rather than this layer swallowing it.
        IgnorePointer(
          child: FadeTransition(
            opacity: _appearAnimation,
            child: ColoredBox(
              color: widget.scrimColor ?? palette.function.neutral.scrim,
            ),
          ),
        ),
        Align(
          alignment: Alignment.bottomCenter,
          child: AnimatedBuilder(
            animation: _appearAnimation,
            builder: (context, child) => Transform.translate(
              offset: Offset(0, _dragOffset),
              child: FractionalTranslation(
                // 1 sits a full sheet height below the bottom edge, 0 at rest.
                translation: Offset(0, 1 - _appearAnimation.value),
                child: child,
              ),
            ),
            child: _sheet(mediaQuery, palette),
          ),
        ),
      ],
    );
  }

  Widget _sheet(MediaQueryData mediaQuery, SemanticPalette palette) {
    final maxSheetHeight = mediaQuery.size.height - mediaQuery.viewPadding.top;
    final bottomPadding = math.max(
      mediaQuery.viewPadding.bottom,
      mediaQuery.viewInsets.bottom,
    );
    final contentPadding = (widget.contentPadding ?? _tokens.contentPadding)
        .add(EdgeInsets.only(bottom: bottomPadding));
    final handleExtent = widget.enableDrag ? _tokens.handleExtent : 0.0;
    var cardMaxHeight = maxSheetHeight - handleExtent;
    if (cardMaxHeight <= 0) {
      cardMaxHeight = maxSheetHeight;
    }

    return GestureDetector(
      behavior: .opaque,
      onVerticalDragUpdate: _handleVerticalDragUpdate,
      onVerticalDragEnd: _handleVerticalDragEnd,
      onVerticalDragCancel: _handleVerticalDragCancel,
      child: SizedBox(
        width: double.infinity,
        child: Column(
          mainAxisSize: .min,
          crossAxisAlignment: .stretch,
          children: [
            if (widget.enableDrag)
              SizedBox(
                height: handleExtent,
                child: Align(
                  alignment: Alignment.bottomCenter,
                  child: Padding(
                    padding: EdgeInsets.only(bottom: _tokens.handleGap),
                    child: _BottomSheetHandle(
                      color: palette.backgroundElevated.primary,
                    ),
                  ),
                ),
              ),
            Flexible(
              fit: .loose,
              child: ConstrainedBox(
                constraints: BoxConstraints(maxHeight: cardMaxHeight),
                child: ClipRRect(
                  borderRadius: _sheetBorderRadius,
                  child: Material(
                    color: palette.backgroundElevated.primary,
                    child: SingleChildScrollView(
                      padding: contentPadding,
                      child: widget.builder(context),
                    ),
                  ),
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _BottomSheetHandle extends StatelessWidget {
  const _BottomSheetHandle({required this.color});

  final Color color;

  @override
  Widget build(BuildContext context) {
    const tokens = BottomSheetTokens.standard;
    return Container(
      width: tokens.handleWidth,
      height: tokens.handleHeight,
      decoration: BoxDecoration(
        color: color,
        borderRadius: BorderRadius.circular(CornerRadius.full),
      ),
    );
  }
}

typedef AsyncAction = FutureOr<void> Function(BuildContext context);

/// The body every confirm and notice modal carries: a title, an optional
/// description, and up to two stacked actions. Each action pops the modal once
/// its callback has run.
///
/// Presentation-agnostic, so [showAdaptiveModal] can hand it to a sheet or to a
/// dialog card unchanged.
class AdaptiveDialogContent extends StatelessWidget {
  const AdaptiveDialogContent({
    super.key,
    this.title,
    this.description,
    this.primaryActionText,
    this.onPrimaryAction,
    this.secondaryActionText,
    this.onSecondaryAction,
    this.titleAlignment = .center,
    this.descriptionAlignment = .center,
    this.primaryType = ButtonType.primary,
    this.primaryTone = ButtonTone.normal,
    this.secondaryType = ButtonType.secondary,
    this.secondaryTone = ButtonTone.normal,
  });

  final String? title;
  final String? description;
  final String? primaryActionText;
  final AsyncAction? onPrimaryAction;
  final String? secondaryActionText;
  final AsyncAction? onSecondaryAction;
  final TextAlign titleAlignment;
  final TextAlign descriptionAlignment;
  final ButtonType primaryType;
  final ButtonTone primaryTone;
  final ButtonType secondaryType;
  final ButtonTone secondaryTone;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);

    return Column(
      mainAxisSize: .min,
      crossAxisAlignment: .stretch,
      children: [
        if (title != null)
          Text(
            title!,
            style: typeScale.header.regular.style(
              color: palette.text.primary,
              weight: Weight.emphasized,
            ),
            textAlign: titleAlignment,
          ),
        if (description != null) ...[
          const SizedBox(height: S.s16),
          Text(
            description!,
            style: typeScale.body.s.style(color: palette.text.secondary),
            textAlign: descriptionAlignment,
          ),
        ],
        const SizedBox(height: S.s24),
        if (primaryActionText != null)
          Button(
            type: primaryType,
            tone: primaryTone,
            onPressed: () async {
              final navigator = Navigator.of(context);
              if (onPrimaryAction != null) {
                await onPrimaryAction!(context);
              }
              if (navigator.mounted) {
                navigator.pop(true);
              }
            },
            label: primaryActionText!,
          ),
        if (secondaryActionText != null) ...[
          const SizedBox(height: S.s16),
          Button(
            type: secondaryType,
            tone: secondaryTone,
            onPressed: () async {
              final navigator = Navigator.of(context);
              if (onSecondaryAction != null) {
                await onSecondaryAction!(context);
              }
              if (navigator.mounted) {
                navigator.pop(true);
              }
            },
            label: secondaryActionText!,
          ),
        ],
      ],
    );
  }
}

/// Asks for a single confirmation on the surface the device calls for, and
/// resolves to whether the primary action was taken.
///
/// Named for the confirmation rather than for the surface: `showAdaptiveDialog`
/// is Flutter's own Material/Cupertino switch, which every caller here imports.
Future<bool> showAdaptiveConfirm({
  required BuildContext context,
  required String title,
  String? description,
  required String primaryActionText,
  FutureOr<void> Function(BuildContext context)? onPrimaryAction,
  ButtonTone primaryTone = ButtonTone.normal,
  EdgeInsetsGeometry? contentPadding,
  bool isDismissible = true,
  bool enableDrag = true,
  Color? barrierColor,
  TextAlign titleAlignment = .center,
  TextAlign descriptionAlignment = .center,
}) async {
  final result = await showAdaptiveModal<bool>(
    context: context,
    builder: (modalContext) => AdaptiveDialogContent(
      title: title,
      description: description,
      primaryActionText: primaryActionText,
      onPrimaryAction: onPrimaryAction,
      titleAlignment: titleAlignment,
      descriptionAlignment: descriptionAlignment,
      primaryTone: primaryTone,
    ),
    contentPadding: contentPadding,
    isDismissible: isDismissible,
    enableDrag: enableDrag,
    barrierColor: barrierColor,
  );
  return result ?? false;
}
