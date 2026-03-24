// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ui/theme/scale.dart';
import 'package:flutter/widgets.dart';
import 'package:air/user/user.dart';
import 'package:provider/provider.dart';

/// Scales the child's interface by keeping the same size
///
/// The scale factor is taken from the [`UserSettingsCubit`].
class InterfaceScale extends StatelessWidget {
  const InterfaceScale({required this.child, super.key});

  final Widget child;

  @override
  Widget build(BuildContext context) {
    final mediaQuery = MediaQuery.of(context);

    final userDefinedUiScaleFactor = context.select(
      (UserSettingsCubit cubit) => cubit.state.interfaceScale ?? 1.0,
    );

    final scalingFactors = getScalingFactors(context);

    final uiScalingFactor = scalingFactors.uiFactor * userDefinedUiScaleFactor;
    final textScaleFactor = scalingFactors.textFactor;

    final wrappedChild = textScaleFactor == 1.0
        ? child
        : MediaQuery(
            data: mediaQuery.copyWith(
              textScaler: TextScaler.linear(textScaleFactor),
            ),
            child: child,
          );

    return uiScalingFactor == 1.0
        ? wrappedChild
        : FractionallySizedBox(
            widthFactor: 1 / uiScalingFactor,
            heightFactor: 1 / uiScalingFactor,
            child: Transform.scale(scale: uiScalingFactor, child: wrappedChild),
          );
  }
}
