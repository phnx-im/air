// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/attachments/attachments.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import '../helpers.dart';
import '../mocks.dart';

const physicalSize = Size(1800, 2000);

final file = UiAttachment(
  attachmentId: 42.attachmentId(),
  filename: 'example.png',
  contentType: 'image/png',
  size: 10 * 1024 * 1024,
  imageMetadata: const UiImageMetadata(
    blurhash: "LEHLk~WB2yk8pyo0adR*.7kCMdnj",
    width: 100,
    height: 50,
  ),
);

List<(Color, Color)> testColors(BuildContext context) {
  final colors = CustomColorScheme.of(context);
  return [
    (colors.message.selfText, colors.message.selfBackground),
    (colors.message.otherText, colors.message.otherBackground),
  ];
}

Map<AttachmentId, UiAttachmentStatus> testStatuses = {
  1.attachmentId(): const UiAttachmentStatus.pending(),
  2.attachmentId(): UiAttachmentStatus.progress(BigInt.from(7 * 1024 * 1024)),
  3.attachmentId(): const UiAttachmentStatus.failed(),
  4.attachmentId(): const UiAttachmentStatus.completed(),
};

class _ImageTestBubble extends StatelessWidget {
  const _ImageTestBubble({
    required this.attachmentId,
    required this.color,
    required this.backgroundColor,
  });

  final AttachmentId attachmentId;
  final Color color;
  final Color backgroundColor;

  @override
  Widget build(BuildContext context) {
    return ClipRRect(
      borderRadius: BorderRadius.circular(Spacings.sm),
      child: Container(
        constraints: const BoxConstraints(maxHeight: 300, maxWidth: 300),
        decoration: BoxDecoration(
          color: backgroundColor,
          borderRadius: BorderRadius.circular(Spacings.sm),
        ),
        child: AttachmentImage(
          attachment: file.copyWith(attachmentId: attachmentId),
          imageMetadata: file.imageMetadata!,
          fit: BoxFit.cover,
        ),
      ),
    );
  }
}

void main() {
  setUpAll(() {
    registerFallbackValue(0.attachmentId());
  });

  late MockAttachmentsRepository attachmentsRepository;

  setUp(() async {
    attachmentsRepository = MockAttachmentsRepository();
    when(
      () => attachmentsRepository.statusStream(
        attachmentId: any(named: 'attachmentId'),
      ),
    ).thenAnswer(
      (invocation) => Stream.value(
        testStatuses[invocation.namedArguments[#attachmentId] as AttachmentId]!,
      ),
    );
  });

  group('AttachmentImage', () {
    Widget buildSubject() => Builder(
      builder: (context) {
        return RepositoryProvider<AttachmentsRepository>.value(
          value: attachmentsRepository,
          child: MaterialApp(
            debugShowCheckedModeBanner: false,
            theme: themeData(MediaQuery.platformBrightnessOf(context)),
            localizationsDelegates: AppLocalizations.localizationsDelegates,
            home: Scaffold(
              body: Padding(
                padding: const EdgeInsets.all(Spacings.s),
                child: SizedBox(
                  width: double.infinity,
                  child: Row(
                    spacing: Spacings.s,
                    children: [
                      for (final (color, backgroundColor) in testColors(
                        context,
                      ))
                        Column(
                          spacing: Spacings.s,
                          crossAxisAlignment: .end,
                          children: [
                            for (final attachmentId in testStatuses.keys) ...[
                              // Container(
                              //   width: 100,
                              //   height: 100,
                              //   decoration: BoxDecoration(
                              //     border: Border.all(color: Colors.yellow),
                              //   ),
                              // ),
                              _ImageTestBubble(
                                attachmentId: attachmentId,
                                color: color,
                                backgroundColor: backgroundColor,
                              ),
                            ],
                          ],
                        ),
                    ],
                  ),
                ),
              ),
            ),
          ),
        );
      },
    );

    testWidgets('renders correctly', (tester) async {
      tester.platformDispatcher.views.first.physicalSize = physicalSize;
      addTearDown(() {
        tester.platformDispatcher.views.first.resetPhysicalSize();
      });

      await tester.pumpWidget(buildSubject());
      await tester.pumpAndSettle();

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/attachment_image.png'),
      );
    });

    testWidgets('renders correctly (dark mode)', (tester) async {
      tester.platformDispatcher.views.first.physicalSize = physicalSize;
      tester.platformDispatcher.platformBrightnessTestValue = Brightness.dark;
      addTearDown(() {
        tester.platformDispatcher.views.first.resetPhysicalSize();
        tester.platformDispatcher.clearPlatformBrightnessTestValue();
      });

      await tester.pumpWidget(buildSubject());
      await tester.pumpAndSettle();

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/attachment_image_dark_mode.png'),
      );
    });
  });
}
