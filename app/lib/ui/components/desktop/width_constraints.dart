import 'package:flutter/widgets.dart';

/// A widget that constrains only the width of its child to 800 logical pixels.
/// Height remains unconstrained and is determined by the child.
class ConstrainedWidth extends StatelessWidget {
  final Widget child;

  const ConstrainedWidth({super.key, required this.child});

  @override
  Widget build(BuildContext context) {
    return Center(child: SizedBox(width: 800, child: child));
  }
}
