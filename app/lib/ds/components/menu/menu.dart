// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:math' as math;

import 'package:air/ds/components/menu/menu_tokens.dart';
import 'package:air/ds/components/state_layer/state_layer.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/widgets.dart';

/// One entry of a [Menu]: a row, or the hairline between two groups of rows.
///
/// A row is an optional leading glyph, a label, and an optional trailing slot.
/// [content] replaces that layout wholesale for a row that carries something
/// else, and [subItems] turns the row into the trigger for a nested menu.
@immutable
class MenuItem {
  const MenuItem({
    String? label,
    AppIconType? icon,
    Widget? leading,
    Widget? trailing,
    Widget? content,
    List<MenuItem> subItems = const <MenuItem>[],
    VoidCallback? onPressed,
    bool destructive = false,
    bool selected = false,
  }) : this._(
         label: label,
         icon: icon,
         leading: leading,
         trailing: trailing,
         content: content,
         subItems: subItems,
         onPressed: onPressed,
         destructive: destructive,
         selected: selected,
         isSeparator: false,
       );

  /// A hairline splitting the rows above from the rows below, for an action
  /// that shouldn't sit in the same group as its neighbours.
  const MenuItem.separator()
    : this._(
        label: null,
        icon: null,
        leading: null,
        trailing: null,
        content: null,
        subItems: const <MenuItem>[],
        onPressed: null,
        destructive: false,
        selected: false,
        isSeparator: true,
      );

  const MenuItem._({
    required this.label,
    required this.icon,
    required this.leading,
    required this.trailing,
    required this.content,
    required this.subItems,
    required this.onPressed,
    required this.destructive,
    required this.selected,
    required this.isSeparator,
  });

  final String? label;

  /// Leading glyph the menu tints and sizes itself. Ignored when [leading] is
  /// set, which is the slot for a glyph the host colors.
  final AppIconType? icon;

  final Widget? leading;

  /// Pinned to the row's trailing edge. The row then fills the card's width and
  /// the label takes the slack.
  final Widget? trailing;

  /// Replaces the leading + label + trailing layout. The menu still supplies
  /// the wash, the selected fill, and the row padding around it.
  final Widget? content;

  /// Nested items. A row that has them opens them in a menu beside this one on
  /// tap or hover, and never fires [onPressed].
  final List<MenuItem> subItems;

  final VoidCallback? onPressed;

  /// Whether the label and the tinted [icon] take the danger color.
  final bool destructive;

  /// Whether the row carries the persistent selected fill.
  final bool selected;

  final bool isSeparator;

  bool get hasSubmenu => subItems.isNotEmpty;

  /// The two fields a host rewrites: [onPressed] to run its own teardown before
  /// the item's action, [subItems] to do the same one level down.
  MenuItem copyWith({VoidCallback? onPressed, List<MenuItem>? subItems}) =>
      MenuItem._(
        label: label,
        icon: icon,
        leading: leading,
        trailing: trailing,
        content: content,
        subItems: subItems ?? this.subItems,
        onPressed: onPressed ?? this.onPressed,
        destructive: destructive,
        selected: selected,
        isSeparator: isSeparator,
      );
}

/// A vertical menu of items on an elevated card.
///
/// The card hugs its widest row, down to [MenuTokens.minWidth]. Rows route
/// hover / press / focus through the shared [StateLayer], shaped by the
/// platform -- touch dips, a pointer washes -- and a selected row carries a
/// persistent fill instead. Past [maxHeight] the rows scroll.
///
/// The menu is a plain widget: hosts that float it over the app go through
/// `showOverlayMenu`, which owns the anchoring and the dismissal.
class Menu extends StatefulWidget {
  const Menu({
    super.key,
    required this.tokens,
    required this.items,
    this.maxHeight,
  });

  final MenuTokens tokens;
  final List<MenuItem> items;

  /// Height ceiling for the card. Past it the rows scroll, so a long menu near
  /// a viewport edge stays reachable instead of running off it.
  final double? maxHeight;

  @override
  State<Menu> createState() => _MenuState();
}

class _MenuState extends State<Menu> {
  /// A submenu aligns to the card rather than to the row that opened it, so the
  /// two cards read as one surface stepping sideways.
  final GlobalKey _cardKey = GlobalKey();

  /// The row whose submenu is open, so opening one closes the last.
  final ValueNotifier<int?> _openSubmenu = ValueNotifier<int?>(null);

  @override
  void dispose() {
    _openSubmenu.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);
    final tokens = widget.tokens;

    final column = Column(
      mainAxisSize: .min,
      crossAxisAlignment: .stretch,
      children: [
        for (var i = 0; i < widget.items.length; i++)
          Padding(
            padding: EdgeInsets.only(top: _gapAbove(i)),
            child: _entry(i),
          ),
      ],
    );

    final maxHeight = widget.maxHeight;
    final body = maxHeight == null
        ? column
        : ConstrainedBox(
            constraints: BoxConstraints(maxHeight: maxHeight),
            child: SingleChildScrollView(child: column),
          );

    return IntrinsicWidth(
      child: Container(
        key: _cardKey,
        constraints: BoxConstraints(minWidth: tokens.minWidth),
        padding: tokens.padding,
        decoration: BoxDecoration(
          color: palette.backgroundElevated.primary,
          borderRadius: BorderRadius.circular(tokens.radius),
          boxShadow: Effect.elevation(MenuTokens.elevation),
        ),
        child: DefaultTextStyle.merge(
          style: typeScale.body.s.style(color: palette.text.primary),
          child: body,
        ),
      ),
    );
  }

  /// A separator's own symmetric gap already spaces the rows around it, so
  /// neither it nor the row after it adds a second one.
  double _gapAbove(int index) {
    if (index == 0 || widget.items[index].isSeparator) return 0;
    return widget.items[index - 1].isSeparator ? 0 : widget.tokens.itemGap;
  }

  Widget _entry(int index) {
    final item = widget.items[index];
    if (item.isSeparator) {
      return Padding(
        padding: const EdgeInsets.symmetric(vertical: MenuTokens.separatorGap),
        child: SizedBox(
          height: MenuTokens.separatorWidth,
          child: ColoredBox(
            color: SemanticPalette.of(context).separator.secondary,
          ),
        ),
      );
    }
    if (item.hasSubmenu) {
      return _SubmenuRow(
        tokens: widget.tokens,
        item: item,
        index: index,
        openSubmenu: _openSubmenu,
        cardKey: _cardKey,
        maxHeight: widget.maxHeight,
      );
    }
    return _MenuRow(tokens: widget.tokens, item: item, onTap: item.onPressed);
  }
}

class _MenuRow extends StatelessWidget {
  const _MenuRow({
    required this.tokens,
    required this.item,
    required this.onTap,
    this.trailing,
    this.highlighted = false,
  });

  final MenuTokens tokens;
  final MenuItem item;
  final VoidCallback? onTap;

  /// Overrides [MenuItem.trailing], for the chevron a submenu row carries.
  final Widget? trailing;

  /// Whether the row takes the selected fill without being [MenuItem.selected]:
  /// an open submenu keeps its trigger lit for as long as it's up.
  final bool highlighted;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);
    final filled = highlighted || item.selected;

    return StateLayer(
      onTap: onTap,
      enabled: onTap != null,
      borderRadius: MenuTokens.itemRadius,
      surface: palette.backgroundElevated.primary,
      // A filled row already carries its fill, so the hover wash would double
      // up on it.
      selected: filled,
      background: DecoratedBox(
        decoration: BoxDecoration(
          color: filled ? palette.fill.tertiary : null,
          borderRadius: BorderRadius.circular(MenuTokens.itemRadius),
        ),
      ),
      child: Padding(padding: MenuTokens.itemPadding, child: _content(palette)),
    );
  }

  Widget _content(SemanticPalette palette) {
    final content = item.content;
    if (content != null) return content;

    final color = item.destructive
        ? palette.function.danger
        : palette.text.primary;
    final trailing = this.trailing ?? item.trailing;
    final icon = item.icon;
    final leading =
        item.leading ??
        (icon == null
            ? null
            : AppIcon(type: icon, size: tokens.iconSize, color: color));
    final label = Text(
      item.label ?? '',
      style: typeScale.body.s.style(color: color, tight: true),
      maxLines: 1,
      softWrap: false,
      overflow: .ellipsis,
    );

    return Row(
      // A trailing widget pins to the row's end, so the row fills the card and
      // the label takes the slack. Without one the row hugs its content.
      mainAxisSize: trailing != null ? .max : .min,
      children: [
        if (leading != null) ...[
          leading,
          const SizedBox(width: MenuTokens.iconGap),
        ],
        if (trailing != null) ...[
          Expanded(child: label),
          const SizedBox(width: MenuTokens.iconGap),
          trailing,
        ] else
          label,
      ],
    );
  }
}

/// A row that opens its [MenuItem.subItems] in a second card beside the first.
class _SubmenuRow extends StatefulWidget {
  const _SubmenuRow({
    required this.tokens,
    required this.item,
    required this.index,
    required this.openSubmenu,
    required this.cardKey,
    this.maxHeight,
  });

  final MenuTokens tokens;
  final MenuItem item;
  final int index;
  final ValueNotifier<int?> openSubmenu;
  final GlobalKey cardKey;
  final double? maxHeight;

  @override
  State<_SubmenuRow> createState() => _SubmenuRowState();
}

class _SubmenuRowState extends State<_SubmenuRow> {
  final OverlayPortalController _controller = OverlayPortalController();
  bool _open = false;

  @override
  void initState() {
    super.initState();
    widget.openSubmenu.addListener(_syncOpenState);
  }

  @override
  void dispose() {
    widget.openSubmenu.removeListener(_syncOpenState);
    super.dispose();
  }

  /// We drive this from the notifier rather than the tap directly, so that
  /// opening one submenu closes whichever one was up. It only ever runs from a
  /// gesture, which is outside the build and layout phases the portal forbids.
  void _syncOpenState() {
    final open = widget.openSubmenu.value == widget.index;
    if (open == _open) return;
    setState(() => _open = open);
    if (open) {
      _controller.show();
    } else {
      _controller.hide();
    }
  }

  void _show() => widget.openSubmenu.value = widget.index;

  void _hide() {
    if (widget.openSubmenu.value == widget.index) {
      widget.openSubmenu.value = null;
    }
  }

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);

    return MouseRegion(
      // A pointer opens the submenu on its way past the row, touch waits for
      // the tap.
      onEnter: DeviceType.isPhone ? null : (_) => _show(),
      child: OverlayPortal(
        controller: _controller,
        overlayChildBuilder: _buildSubmenu,
        child: _MenuRow(
          tokens: widget.tokens,
          item: widget.item,
          onTap: _open ? _hide : _show,
          highlighted: _open,
          trailing: AppIcon(
            type: AppIconType.chevronRight,
            size: widget.tokens.iconSize,
            color: palette.text.tertiary,
          ),
        ),
      ),
    );
  }

  Widget _buildSubmenu(BuildContext context) {
    final items = [
      for (final item in widget.item.subItems)
        if (item.onPressed case final onPressed?)
          item.copyWith(
            onPressed: () {
              _hide();
              onPressed();
            },
          )
        else
          item,
    ];

    return Stack(
      children: [
        // Translucent, so the parent card behind it stays hoverable while the
        // submenu is up and a tap anywhere else closes the submenu alone.
        Positioned.fill(
          child: GestureDetector(behavior: .translucent, onTap: _hide),
        ),
        CustomSingleChildLayout(
          delegate: _SubmenuLayout(
            anchor: _cardRect(context) ?? Rect.zero,
            gap: MenuTokens.submenuGap,
          ),
          child: Menu(
            tokens: widget.tokens,
            items: items,
            maxHeight: widget.maxHeight,
          ),
        ),
      ],
    );
  }

  /// The parent card's bounds in the overlay's coordinate space, which is where
  /// the submenu is laid out.
  Rect? _cardRect(BuildContext overlayContext) {
    final overlay = Overlay.of(overlayContext).context.findRenderObject();
    final card = widget.cardKey.currentContext?.findRenderObject();
    if (overlay is! RenderBox || card is! RenderBox || !card.hasSize) {
      return null;
    }
    return card.localToGlobal(Offset.zero, ancestor: overlay) & card.size;
  }
}

/// Places a submenu beside the card that opened it, tops aligned.
class _SubmenuLayout extends SingleChildLayoutDelegate {
  const _SubmenuLayout({required this.anchor, required this.gap});

  final Rect anchor;
  final double gap;

  @override
  BoxConstraints getConstraintsForChild(BoxConstraints constraints) =>
      constraints.loosen();

  @override
  Offset getPositionForChild(Size size, Size childSize) {
    final trailing = anchor.right + gap;
    final leading = anchor.left - gap - childSize.width;
    final dx = trailing + childSize.width <= size.width ? trailing : leading;
    return Offset(
      dx.clamp(0.0, math.max(0.0, size.width - childSize.width)),
      anchor.top.clamp(0.0, math.max(0.0, size.height - childSize.height)),
    );
  }

  @override
  bool shouldRelayout(_SubmenuLayout oldDelegate) =>
      oldDelegate.anchor != anchor || oldDelegate.gap != gap;
}
