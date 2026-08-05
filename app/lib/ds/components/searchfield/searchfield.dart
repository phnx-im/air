// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/searchfield/searchfield_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/material.dart';

/// A pill carrying a query: the search glyph, the text, and a clear button
/// that appears once there's something to clear.
///
/// The host owns the query. Pass a [controller] to seed or read it, and follow
/// it through [onChanged], which also fires when the clear button empties the
/// field.
class SearchField extends StatefulWidget {
  const SearchField({
    super.key,
    required this.tokens,
    this.controller,
    this.focusNode,
    this.hintText,
    this.onChanged,
    this.onSubmitted,
    this.autofocus = false,
    this.textInputAction,
  });

  final SearchFieldTokens tokens;

  /// The host's controller. Without one the field keeps its own, which is
  /// enough when the host only follows the query through [onChanged].
  final TextEditingController? controller;

  final FocusNode? focusNode;
  final String? hintText;

  final ValueChanged<String>? onChanged;
  final ValueChanged<String>? onSubmitted;

  final bool autofocus;
  final TextInputAction? textInputAction;

  @override
  State<SearchField> createState() => _SearchFieldState();
}

class _SearchFieldState extends State<SearchField> {
  TextEditingController? _fallbackController;

  TextEditingController get _controller =>
      widget.controller ?? (_fallbackController ??= TextEditingController());

  @override
  void dispose() {
    _fallbackController?.dispose();
    super.dispose();
  }

  void _clear() {
    _controller.clear();
    // A programmatic edit never reaches onChanged, so the host would otherwise
    // keep filtering on the query it can no longer see.
    widget.onChanged?.call('');
  }

  @override
  Widget build(BuildContext context) {
    final tokens = widget.tokens;
    final palette = SemanticPalette.of(context);
    final inputStyle = typeScale.body.regular.style(
      color: palette.text.primary,
      tight: true,
    );

    return Container(
      padding: SearchFieldTokens.padding,
      decoration: BoxDecoration(
        color: palette.fill.tertiary,
        borderRadius: BorderRadius.circular(SearchFieldTokens.radius),
      ),
      child: Row(
        children: [
          AppIcon.search(size: tokens.iconSize, color: palette.text.tertiary),
          const SizedBox(width: SearchFieldTokens.gap),
          Expanded(
            child: TextField(
              controller: _controller,
              focusNode: widget.focusNode,
              autofocus: widget.autofocus,
              textInputAction: widget.textInputAction,
              onChanged: widget.onChanged,
              onSubmitted: widget.onSubmitted,
              style: inputStyle,
              // Lock the line box to the input style so the pill doesn't grow
              // when the first character replaces the hint.
              strutStyle: StrutStyle.fromTextStyle(
                inputStyle,
                forceStrutHeight: true,
              ),
              // The pill is the chrome, so the field itself draws nothing. We
              // spell out the borders and fill rather than let the ambient
              // input theme add its own.
              decoration: InputDecoration(
                isDense: true,
                contentPadding: EdgeInsets.zero,
                filled: false,
                border: InputBorder.none,
                enabledBorder: InputBorder.none,
                disabledBorder: InputBorder.none,
                focusedBorder: InputBorder.none,
                errorBorder: InputBorder.none,
                focusedErrorBorder: InputBorder.none,
                hintText: widget.hintText,
                hintStyle: inputStyle.copyWith(color: palette.text.tertiary),
              ),
            ),
          ),
          ValueListenableBuilder<TextEditingValue>(
            valueListenable: _controller,
            builder: (context, value, _) => value.text.isEmpty
                ? const SizedBox.shrink()
                : _buildClearButton(palette),
          ),
        ],
      ),
    );
  }

  /// The gap sits inside the gesture, so the target is wider than the glyph
  /// without the glyph moving.
  Widget _buildClearButton(SemanticPalette palette) => GestureDetector(
    behavior: HitTestBehavior.opaque,
    onTap: _clear,
    child: Padding(
      padding: const EdgeInsets.only(left: SearchFieldTokens.gap),
      child: AppIcon.x(
        size: widget.tokens.clearSize,
        color: palette.text.tertiary,
      ),
    ),
  );
}
