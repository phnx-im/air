// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';
import 'dart:math' as math;

import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:flutter/material.dart';

/// Displays a custom bottom sheet modal that slides and fades into view.
///
/// The sheet adapts to its content up to a configurable `maxHeight` (or
/// `maxHeightFraction` of the available screen height) and can be dismissed by
/// tapping the barrier or dragging the handle downward.
Future<T?> showBottomSheetModal<T>({
  required BuildContext context,
  required WidgetBuilder builder,
  double? maxHeight,
  double maxHeightFraction = 0.4,
  EdgeInsetsGeometry? contentPadding,
  EdgeInsetsGeometry? margin,
  bool isDismissible = true,
  bool enableDrag = true,
  Duration animationDuration = const Duration(milliseconds: 280),
  Color? barrierColor,
}) {
  final color =
      barrierColor ??
      Colors.black.withValues(
        alpha: Theme.of(context).brightness == Brightness.dark ? 0.55 : 0.35,
      );

  return showGeneralDialog<T>(
    context: context,
    barrierDismissible: isDismissible,
    barrierLabel: MaterialLocalizations.of(context).modalBarrierDismissLabel,
    barrierColor: color,
    transitionDuration: animationDuration,
    transitionBuilder: (context, animation, secondaryAnimation, child) => child,
    pageBuilder: (context, animation, secondaryAnimation) {
      return _BottomSheetModal(
        animation: animation,
        builder: builder,
        enableDrag: enableDrag,
        isDismissible: isDismissible,
        maxHeight: maxHeight,
        maxHeightFraction: maxHeightFraction,
        contentPadding: contentPadding,
        margin: margin,
      );
    },
  );
}

class _BottomSheetModal extends StatefulWidget {
  const _BottomSheetModal({
    required this.animation,
    required this.builder,
    required this.enableDrag,
    required this.isDismissible,
    required this.maxHeight,
    required this.maxHeightFraction,
    this.contentPadding,
    this.margin,
  });

  final Animation<double> animation;
  final WidgetBuilder builder;
  final bool enableDrag;
  final bool isDismissible;
  final double? maxHeight;
  final double maxHeightFraction;
  final EdgeInsetsGeometry? contentPadding;
  final EdgeInsetsGeometry? margin;

  @override
  State<_BottomSheetModal> createState() => _BottomSheetModalState();
}

class _BottomSheetModalState extends State<_BottomSheetModal>
    with SingleTickerProviderStateMixin {
  static const _sheetBorderRadius = BorderRadius.vertical(
    top: Radius.circular(28),
  );
  static const double _handleHeight = 4;
  static const double _handleTopSpacing = Spacings.xxs;

  late final CurvedAnimation _appearAnimation;
  late final Animation<double> _slideAnimation;

  late final AnimationController _dragResetController;
  Animation<double>? _dragResetAnimation;
  VoidCallback? _dragResetListener;

  double _dragOffset = 0;

  @override
  void initState() {
    super.initState();

    _appearAnimation = CurvedAnimation(
      parent: widget.animation,
      curve: Curves.easeOutCubic,
      reverseCurve: Curves.easeInCubic,
    );
    _slideAnimation = Tween<double>(
      begin: 48,
      end: 0,
    ).animate(_appearAnimation);

    _dragResetController = AnimationController(
      vsync: this,
      duration: const Duration(milliseconds: 180),
    );
  }

  @override
  void dispose() {
    _detachDragResetListener();
    _dragResetController.dispose();
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
      CurvedAnimation(parent: _dragResetController, curve: Curves.easeOutCubic),
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
    if (velocity > 700 || _dragOffset > 120) {
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
    final availableHeight =
        widget.maxHeight ?? mediaQuery.size.height * widget.maxHeightFraction;
    final maxSheetHeight = math.min(
      availableHeight,
      mediaQuery.size.height - mediaQuery.viewPadding.top,
    );
    final bottomPadding = math.max(
      mediaQuery.viewPadding.bottom,
      mediaQuery.viewInsets.bottom,
    );
    final colorScheme = CustomColorScheme.of(context);
    final basePadding =
        widget.contentPadding ??
        const EdgeInsets.fromLTRB(
          Spacings.m,
          Spacings.l,
          Spacings.m,
          Spacings.xxs,
        );
    final contentPadding = basePadding.add(
      EdgeInsets.only(bottom: bottomPadding),
    );
    final handleHitExtent = widget.enableDrag
        ? _handleHeight + (_handleTopSpacing * 2)
        : 0.0;
    var sheetMaxHeight = maxSheetHeight - handleHitExtent;
    if (sheetMaxHeight <= 0) {
      sheetMaxHeight = maxSheetHeight;
    }

    return Align(
      alignment: Alignment.bottomCenter,
      child: Padding(
        padding: widget.margin ?? EdgeInsets.zero,
        child: AnimatedBuilder(
          animation: _appearAnimation,
          builder: (context, child) {
            final opacity = _appearAnimation.value.clamp(0.0, 1.0);
            final translateY = _slideAnimation.value + _dragOffset;
            return Opacity(
              opacity: opacity,
              child: Transform.translate(
                offset: Offset(0, translateY),
                child: child,
              ),
            );
          },
          child: GestureDetector(
            behavior: HitTestBehavior.opaque,
            onVerticalDragUpdate: _handleVerticalDragUpdate,
            onVerticalDragEnd: _handleVerticalDragEnd,
            onVerticalDragCancel: _handleVerticalDragCancel,
            child: SizedBox(
              width: double.infinity,
              child: Column(
                mainAxisSize: MainAxisSize.min,
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  if (widget.enableDrag)
                    SizedBox(
                      height: handleHitExtent,
                      child: Align(
                        alignment: Alignment.bottomCenter,
                        child: Padding(
                          padding: const EdgeInsets.only(
                            bottom: _handleTopSpacing,
                          ),
                          child: _BottomSheetHandle(
                            color: colorScheme.backgroundElevated.primary,
                          ),
                        ),
                      ),
                    ),
                  Flexible(
                    fit: FlexFit.loose,
                    child: ConstrainedBox(
                      constraints: BoxConstraints(maxHeight: sheetMaxHeight),
                      child: Container(
                        decoration: const BoxDecoration(
                          borderRadius: _sheetBorderRadius,
                        ),
                        child: ClipRRect(
                          borderRadius: _sheetBorderRadius,
                          child: Material(
                            color: colorScheme.backgroundElevated.primary,
                            child: SingleChildScrollView(
                              padding: contentPadding,
                              child: widget.builder(context),
                            ),
                          ),
                        ),
                      ),
                    ),
                  ),
                ],
              ),
            ),
          ),
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
    return Container(
      width: 64,
      height: 4,
      decoration: BoxDecoration(
        color: color,
        borderRadius: BorderRadius.circular(999),
      ),
    );
  }
}

class BottomSheetDialogContent extends StatelessWidget {
  const BottomSheetDialogContent({
    super.key,
    this.title,
    this.description,
    this.primaryActionText,
    this.onPrimaryAction,
    this.titleAlignment = TextAlign.center,
    this.descriptionAlignment = TextAlign.center,
    this.isPrimaryDanger = false,
  });

  final String? title;
  final String? description;
  final String? primaryActionText;
  final FutureOr<void> Function(BuildContext context)? onPrimaryAction;
  final TextAlign titleAlignment;
  final TextAlign descriptionAlignment;
  final bool isPrimaryDanger;

  @override
  Widget build(BuildContext context) {
    final textTheme = Theme.of(context).textTheme;
    final colors = CustomColorScheme.of(context);
    final baseStyle = CustomOutlineButtonStyle(
      colorScheme: colors,
      baselineTextTheme: textTheme,
    );
    final dangerBackground = colors.function.danger;
    final dangerForeground = colors.function.white;
    final buttonStyle = isPrimaryDanger
        ? baseStyle.copyWith(
            backgroundColor: WidgetStateProperty.all(dangerBackground),
            foregroundColor: WidgetStateProperty.all(dangerForeground),
            overlayColor: WidgetStateProperty.all(
              dangerBackground.withValues(alpha: 0.9),
            ),
          )
        : baseStyle;

    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (title != null)
          Text(
            title!,
            style: textTheme.titleLarge!.copyWith(
              fontWeight: FontWeight.bold,
              color: colors.text.primary,
            ),
            textAlign: titleAlignment,
          ),
        if (description != null) ...[
          const SizedBox(height: Spacings.s),
          Text(
            description!,
            style: textTheme.bodyMedium?.copyWith(
              color: colors.text.secondary,
              height: 1.4,
            ),
            textAlign: descriptionAlignment,
          ),
        ],
        const SizedBox(height: Spacings.m),
        if (primaryActionText != null)
          OutlinedButton(
            onPressed: () async {
              final navigator = Navigator.of(context);
              if (onPrimaryAction != null) {
                await onPrimaryAction!(context);
              }
              if (navigator.mounted) {
                navigator.pop(true);
              }
            },
            style: buttonStyle,
            child: Text(
              primaryActionText!,
              style: textTheme.bodyLarge!.copyWith(
                color: buttonStyle.foregroundColor?.resolve({}),
              ),
            ),
          ),
      ],
    );
  }
}

Future<bool> showBottomSheetDialog({
  required BuildContext context,
  required String title,
  String? description,
  required String primaryActionText,
  FutureOr<void> Function(BuildContext context)? onPrimaryAction,
  bool isPrimaryDanger = false,
  double? maxHeight,
  double maxHeightFraction = 0.4,
  EdgeInsetsGeometry? contentPadding,
  EdgeInsetsGeometry? margin,
  bool isDismissible = true,
  bool enableDrag = true,
  Duration animationDuration = const Duration(milliseconds: 280),
  Color? barrierColor,
  TextAlign titleAlignment = TextAlign.center,
  TextAlign descriptionAlignment = TextAlign.center,
}) async {
  final result = await showBottomSheetModal<bool>(
    context: context,
    builder: (sheetContext) => BottomSheetDialogContent(
      title: title,
      description: description,
      primaryActionText: primaryActionText,
      onPrimaryAction: onPrimaryAction,
      titleAlignment: titleAlignment,
      descriptionAlignment: descriptionAlignment,
      isPrimaryDanger: isPrimaryDanger,
    ),
    maxHeight: maxHeight,
    maxHeightFraction: maxHeightFraction,
    contentPadding: contentPadding,
    margin: margin,
    isDismissible: isDismissible,
    enableDrag: enableDrag,
    animationDuration: animationDuration,
    barrierColor: barrierColor,
  );
  return result ?? false;
}
