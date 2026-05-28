// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'message_content.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// dart format off
T _$identity<T>(T value) => value;
/// @nodoc
mixin _$UiAttachment {

 AttachmentId get attachmentId; String get filename; String get contentType; String? get description; int get size; UiImageMetadata? get imageMetadata;
/// Create a copy of UiAttachment
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$UiAttachmentCopyWith<UiAttachment> get copyWith => _$UiAttachmentCopyWithImpl<UiAttachment>(this as UiAttachment, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is UiAttachment&&(identical(other.attachmentId, attachmentId) || other.attachmentId == attachmentId)&&(identical(other.filename, filename) || other.filename == filename)&&(identical(other.contentType, contentType) || other.contentType == contentType)&&(identical(other.description, description) || other.description == description)&&(identical(other.size, size) || other.size == size)&&(identical(other.imageMetadata, imageMetadata) || other.imageMetadata == imageMetadata));
}


@override
int get hashCode => Object.hash(runtimeType,attachmentId,filename,contentType,description,size,imageMetadata);

@override
String toString() {
  return 'UiAttachment(attachmentId: $attachmentId, filename: $filename, contentType: $contentType, description: $description, size: $size, imageMetadata: $imageMetadata)';
}


}

/// @nodoc
abstract mixin class $UiAttachmentCopyWith<$Res>  {
  factory $UiAttachmentCopyWith(UiAttachment value, $Res Function(UiAttachment) _then) = _$UiAttachmentCopyWithImpl;
@useResult
$Res call({
 AttachmentId attachmentId, String filename, String contentType, String? description, int size, UiImageMetadata? imageMetadata
});


$UiImageMetadataCopyWith<$Res>? get imageMetadata;

}
/// @nodoc
class _$UiAttachmentCopyWithImpl<$Res>
    implements $UiAttachmentCopyWith<$Res> {
  _$UiAttachmentCopyWithImpl(this._self, this._then);

  final UiAttachment _self;
  final $Res Function(UiAttachment) _then;

/// Create a copy of UiAttachment
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') @override $Res call({Object? attachmentId = null,Object? filename = null,Object? contentType = null,Object? description = freezed,Object? size = null,Object? imageMetadata = freezed,}) {
  return _then(_self.copyWith(
attachmentId: null == attachmentId ? _self.attachmentId : attachmentId // ignore: cast_nullable_to_non_nullable
as AttachmentId,filename: null == filename ? _self.filename : filename // ignore: cast_nullable_to_non_nullable
as String,contentType: null == contentType ? _self.contentType : contentType // ignore: cast_nullable_to_non_nullable
as String,description: freezed == description ? _self.description : description // ignore: cast_nullable_to_non_nullable
as String?,size: null == size ? _self.size : size // ignore: cast_nullable_to_non_nullable
as int,imageMetadata: freezed == imageMetadata ? _self.imageMetadata : imageMetadata // ignore: cast_nullable_to_non_nullable
as UiImageMetadata?,
  ));
}
/// Create a copy of UiAttachment
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$UiImageMetadataCopyWith<$Res>? get imageMetadata {
    if (_self.imageMetadata == null) {
    return null;
  }

  return $UiImageMetadataCopyWith<$Res>(_self.imageMetadata!, (value) {
    return _then(_self.copyWith(imageMetadata: value));
  });
}
}



/// @nodoc


class _UiAttachment implements UiAttachment {
  const _UiAttachment({required this.attachmentId, required this.filename, required this.contentType, this.description, required this.size, this.imageMetadata});
  

@override final  AttachmentId attachmentId;
@override final  String filename;
@override final  String contentType;
@override final  String? description;
@override final  int size;
@override final  UiImageMetadata? imageMetadata;

/// Create a copy of UiAttachment
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
_$UiAttachmentCopyWith<_UiAttachment> get copyWith => __$UiAttachmentCopyWithImpl<_UiAttachment>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is _UiAttachment&&(identical(other.attachmentId, attachmentId) || other.attachmentId == attachmentId)&&(identical(other.filename, filename) || other.filename == filename)&&(identical(other.contentType, contentType) || other.contentType == contentType)&&(identical(other.description, description) || other.description == description)&&(identical(other.size, size) || other.size == size)&&(identical(other.imageMetadata, imageMetadata) || other.imageMetadata == imageMetadata));
}


@override
int get hashCode => Object.hash(runtimeType,attachmentId,filename,contentType,description,size,imageMetadata);

@override
String toString() {
  return 'UiAttachment(attachmentId: $attachmentId, filename: $filename, contentType: $contentType, description: $description, size: $size, imageMetadata: $imageMetadata)';
}


}

/// @nodoc
abstract mixin class _$UiAttachmentCopyWith<$Res> implements $UiAttachmentCopyWith<$Res> {
  factory _$UiAttachmentCopyWith(_UiAttachment value, $Res Function(_UiAttachment) _then) = __$UiAttachmentCopyWithImpl;
@override @useResult
$Res call({
 AttachmentId attachmentId, String filename, String contentType, String? description, int size, UiImageMetadata? imageMetadata
});


@override $UiImageMetadataCopyWith<$Res>? get imageMetadata;

}
/// @nodoc
class __$UiAttachmentCopyWithImpl<$Res>
    implements _$UiAttachmentCopyWith<$Res> {
  __$UiAttachmentCopyWithImpl(this._self, this._then);

  final _UiAttachment _self;
  final $Res Function(_UiAttachment) _then;

/// Create a copy of UiAttachment
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? attachmentId = null,Object? filename = null,Object? contentType = null,Object? description = freezed,Object? size = null,Object? imageMetadata = freezed,}) {
  return _then(_UiAttachment(
attachmentId: null == attachmentId ? _self.attachmentId : attachmentId // ignore: cast_nullable_to_non_nullable
as AttachmentId,filename: null == filename ? _self.filename : filename // ignore: cast_nullable_to_non_nullable
as String,contentType: null == contentType ? _self.contentType : contentType // ignore: cast_nullable_to_non_nullable
as String,description: freezed == description ? _self.description : description // ignore: cast_nullable_to_non_nullable
as String?,size: null == size ? _self.size : size // ignore: cast_nullable_to_non_nullable
as int,imageMetadata: freezed == imageMetadata ? _self.imageMetadata : imageMetadata // ignore: cast_nullable_to_non_nullable
as UiImageMetadata?,
  ));
}

/// Create a copy of UiAttachment
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$UiImageMetadataCopyWith<$Res>? get imageMetadata {
    if (_self.imageMetadata == null) {
    return null;
  }

  return $UiImageMetadataCopyWith<$Res>(_self.imageMetadata!, (value) {
    return _then(_self.copyWith(imageMetadata: value));
  });
}
}

/// @nodoc
mixin _$UiImageMetadata {

 String get blurhash; int get width; int get height;
/// Create a copy of UiImageMetadata
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$UiImageMetadataCopyWith<UiImageMetadata> get copyWith => _$UiImageMetadataCopyWithImpl<UiImageMetadata>(this as UiImageMetadata, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is UiImageMetadata&&(identical(other.blurhash, blurhash) || other.blurhash == blurhash)&&(identical(other.width, width) || other.width == width)&&(identical(other.height, height) || other.height == height));
}


@override
int get hashCode => Object.hash(runtimeType,blurhash,width,height);

@override
String toString() {
  return 'UiImageMetadata(blurhash: $blurhash, width: $width, height: $height)';
}


}

/// @nodoc
abstract mixin class $UiImageMetadataCopyWith<$Res>  {
  factory $UiImageMetadataCopyWith(UiImageMetadata value, $Res Function(UiImageMetadata) _then) = _$UiImageMetadataCopyWithImpl;
@useResult
$Res call({
 String blurhash, int width, int height
});




}
/// @nodoc
class _$UiImageMetadataCopyWithImpl<$Res>
    implements $UiImageMetadataCopyWith<$Res> {
  _$UiImageMetadataCopyWithImpl(this._self, this._then);

  final UiImageMetadata _self;
  final $Res Function(UiImageMetadata) _then;

/// Create a copy of UiImageMetadata
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') @override $Res call({Object? blurhash = null,Object? width = null,Object? height = null,}) {
  return _then(_self.copyWith(
blurhash: null == blurhash ? _self.blurhash : blurhash // ignore: cast_nullable_to_non_nullable
as String,width: null == width ? _self.width : width // ignore: cast_nullable_to_non_nullable
as int,height: null == height ? _self.height : height // ignore: cast_nullable_to_non_nullable
as int,
  ));
}

}



/// @nodoc


class _UiImageMetadata implements UiImageMetadata {
  const _UiImageMetadata({required this.blurhash, required this.width, required this.height});
  

@override final  String blurhash;
@override final  int width;
@override final  int height;

/// Create a copy of UiImageMetadata
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
_$UiImageMetadataCopyWith<_UiImageMetadata> get copyWith => __$UiImageMetadataCopyWithImpl<_UiImageMetadata>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is _UiImageMetadata&&(identical(other.blurhash, blurhash) || other.blurhash == blurhash)&&(identical(other.width, width) || other.width == width)&&(identical(other.height, height) || other.height == height));
}


@override
int get hashCode => Object.hash(runtimeType,blurhash,width,height);

@override
String toString() {
  return 'UiImageMetadata(blurhash: $blurhash, width: $width, height: $height)';
}


}

/// @nodoc
abstract mixin class _$UiImageMetadataCopyWith<$Res> implements $UiImageMetadataCopyWith<$Res> {
  factory _$UiImageMetadataCopyWith(_UiImageMetadata value, $Res Function(_UiImageMetadata) _then) = __$UiImageMetadataCopyWithImpl;
@override @useResult
$Res call({
 String blurhash, int width, int height
});




}
/// @nodoc
class __$UiImageMetadataCopyWithImpl<$Res>
    implements _$UiImageMetadataCopyWith<$Res> {
  __$UiImageMetadataCopyWithImpl(this._self, this._then);

  final _UiImageMetadata _self;
  final $Res Function(_UiImageMetadata) _then;

/// Create a copy of UiImageMetadata
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? blurhash = null,Object? width = null,Object? height = null,}) {
  return _then(_UiImageMetadata(
blurhash: null == blurhash ? _self.blurhash : blurhash // ignore: cast_nullable_to_non_nullable
as String,width: null == width ? _self.width : width // ignore: cast_nullable_to_non_nullable
as int,height: null == height ? _self.height : height // ignore: cast_nullable_to_non_nullable
as int,
  ));
}


}

/// @nodoc
mixin _$UiMimiContent {

 Uint8List? get replaces; Uint8List get topicId; Uint8List? get inReplyTo; String? get plainBody; MessageContent? get content; List<UiAttachment> get attachments;
/// Create a copy of UiMimiContent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$UiMimiContentCopyWith<UiMimiContent> get copyWith => _$UiMimiContentCopyWithImpl<UiMimiContent>(this as UiMimiContent, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is UiMimiContent&&const DeepCollectionEquality().equals(other.replaces, replaces)&&const DeepCollectionEquality().equals(other.topicId, topicId)&&const DeepCollectionEquality().equals(other.inReplyTo, inReplyTo)&&(identical(other.plainBody, plainBody) || other.plainBody == plainBody)&&(identical(other.content, content) || other.content == content)&&const DeepCollectionEquality().equals(other.attachments, attachments));
}


@override
int get hashCode => Object.hash(runtimeType,const DeepCollectionEquality().hash(replaces),const DeepCollectionEquality().hash(topicId),const DeepCollectionEquality().hash(inReplyTo),plainBody,content,const DeepCollectionEquality().hash(attachments));

@override
String toString() {
  return 'UiMimiContent(replaces: $replaces, topicId: $topicId, inReplyTo: $inReplyTo, plainBody: $plainBody, content: $content, attachments: $attachments)';
}


}

/// @nodoc
abstract mixin class $UiMimiContentCopyWith<$Res>  {
  factory $UiMimiContentCopyWith(UiMimiContent value, $Res Function(UiMimiContent) _then) = _$UiMimiContentCopyWithImpl;
@useResult
$Res call({
 Uint8List? replaces, Uint8List topicId, Uint8List? inReplyTo, String? plainBody, MessageContent? content, List<UiAttachment> attachments
});


$MessageContentCopyWith<$Res>? get content;

}
/// @nodoc
class _$UiMimiContentCopyWithImpl<$Res>
    implements $UiMimiContentCopyWith<$Res> {
  _$UiMimiContentCopyWithImpl(this._self, this._then);

  final UiMimiContent _self;
  final $Res Function(UiMimiContent) _then;

/// Create a copy of UiMimiContent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') @override $Res call({Object? replaces = freezed,Object? topicId = null,Object? inReplyTo = freezed,Object? plainBody = freezed,Object? content = freezed,Object? attachments = null,}) {
  return _then(_self.copyWith(
replaces: freezed == replaces ? _self.replaces : replaces // ignore: cast_nullable_to_non_nullable
as Uint8List?,topicId: null == topicId ? _self.topicId : topicId // ignore: cast_nullable_to_non_nullable
as Uint8List,inReplyTo: freezed == inReplyTo ? _self.inReplyTo : inReplyTo // ignore: cast_nullable_to_non_nullable
as Uint8List?,plainBody: freezed == plainBody ? _self.plainBody : plainBody // ignore: cast_nullable_to_non_nullable
as String?,content: freezed == content ? _self.content : content // ignore: cast_nullable_to_non_nullable
as MessageContent?,attachments: null == attachments ? _self.attachments : attachments // ignore: cast_nullable_to_non_nullable
as List<UiAttachment>,
  ));
}
/// Create a copy of UiMimiContent
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$MessageContentCopyWith<$Res>? get content {
    if (_self.content == null) {
    return null;
  }

  return $MessageContentCopyWith<$Res>(_self.content!, (value) {
    return _then(_self.copyWith(content: value));
  });
}
}



/// @nodoc


class _UiMimiContent implements UiMimiContent {
  const _UiMimiContent({this.replaces, required this.topicId, this.inReplyTo, this.plainBody, this.content, required final  List<UiAttachment> attachments}): _attachments = attachments;
  

@override final  Uint8List? replaces;
@override final  Uint8List topicId;
@override final  Uint8List? inReplyTo;
@override final  String? plainBody;
@override final  MessageContent? content;
 final  List<UiAttachment> _attachments;
@override List<UiAttachment> get attachments {
  if (_attachments is EqualUnmodifiableListView) return _attachments;
  // ignore: implicit_dynamic_type
  return EqualUnmodifiableListView(_attachments);
}


/// Create a copy of UiMimiContent
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
_$UiMimiContentCopyWith<_UiMimiContent> get copyWith => __$UiMimiContentCopyWithImpl<_UiMimiContent>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is _UiMimiContent&&const DeepCollectionEquality().equals(other.replaces, replaces)&&const DeepCollectionEquality().equals(other.topicId, topicId)&&const DeepCollectionEquality().equals(other.inReplyTo, inReplyTo)&&(identical(other.plainBody, plainBody) || other.plainBody == plainBody)&&(identical(other.content, content) || other.content == content)&&const DeepCollectionEquality().equals(other._attachments, _attachments));
}


@override
int get hashCode => Object.hash(runtimeType,const DeepCollectionEquality().hash(replaces),const DeepCollectionEquality().hash(topicId),const DeepCollectionEquality().hash(inReplyTo),plainBody,content,const DeepCollectionEquality().hash(_attachments));

@override
String toString() {
  return 'UiMimiContent(replaces: $replaces, topicId: $topicId, inReplyTo: $inReplyTo, plainBody: $plainBody, content: $content, attachments: $attachments)';
}


}

/// @nodoc
abstract mixin class _$UiMimiContentCopyWith<$Res> implements $UiMimiContentCopyWith<$Res> {
  factory _$UiMimiContentCopyWith(_UiMimiContent value, $Res Function(_UiMimiContent) _then) = __$UiMimiContentCopyWithImpl;
@override @useResult
$Res call({
 Uint8List? replaces, Uint8List topicId, Uint8List? inReplyTo, String? plainBody, MessageContent? content, List<UiAttachment> attachments
});


@override $MessageContentCopyWith<$Res>? get content;

}
/// @nodoc
class __$UiMimiContentCopyWithImpl<$Res>
    implements _$UiMimiContentCopyWith<$Res> {
  __$UiMimiContentCopyWithImpl(this._self, this._then);

  final _UiMimiContent _self;
  final $Res Function(_UiMimiContent) _then;

/// Create a copy of UiMimiContent
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? replaces = freezed,Object? topicId = null,Object? inReplyTo = freezed,Object? plainBody = freezed,Object? content = freezed,Object? attachments = null,}) {
  return _then(_UiMimiContent(
replaces: freezed == replaces ? _self.replaces : replaces // ignore: cast_nullable_to_non_nullable
as Uint8List?,topicId: null == topicId ? _self.topicId : topicId // ignore: cast_nullable_to_non_nullable
as Uint8List,inReplyTo: freezed == inReplyTo ? _self.inReplyTo : inReplyTo // ignore: cast_nullable_to_non_nullable
as Uint8List?,plainBody: freezed == plainBody ? _self.plainBody : plainBody // ignore: cast_nullable_to_non_nullable
as String?,content: freezed == content ? _self.content : content // ignore: cast_nullable_to_non_nullable
as MessageContent?,attachments: null == attachments ? _self._attachments : attachments // ignore: cast_nullable_to_non_nullable
as List<UiAttachment>,
  ));
}

/// Create a copy of UiMimiContent
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$MessageContentCopyWith<$Res>? get content {
    if (_self.content == null) {
    return null;
  }

  return $MessageContentCopyWith<$Res>(_self.content!, (value) {
    return _then(_self.copyWith(content: value));
  });
}
}

/// @nodoc
mixin _$UiMimiId {

 U8Array32 get field0;
/// Create a copy of UiMimiId
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$UiMimiIdCopyWith<UiMimiId> get copyWith => _$UiMimiIdCopyWithImpl<UiMimiId>(this as UiMimiId, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is UiMimiId&&const DeepCollectionEquality().equals(other.field0, field0));
}


@override
int get hashCode => Object.hash(runtimeType,const DeepCollectionEquality().hash(field0));

@override
String toString() {
  return 'UiMimiId(field0: $field0)';
}


}

/// @nodoc
abstract mixin class $UiMimiIdCopyWith<$Res>  {
  factory $UiMimiIdCopyWith(UiMimiId value, $Res Function(UiMimiId) _then) = _$UiMimiIdCopyWithImpl;
@useResult
$Res call({
 U8Array32 field0
});




}
/// @nodoc
class _$UiMimiIdCopyWithImpl<$Res>
    implements $UiMimiIdCopyWith<$Res> {
  _$UiMimiIdCopyWithImpl(this._self, this._then);

  final UiMimiId _self;
  final $Res Function(UiMimiId) _then;

/// Create a copy of UiMimiId
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') @override $Res call({Object? field0 = null,}) {
  return _then(_self.copyWith(
field0: null == field0 ? _self.field0 : field0 // ignore: cast_nullable_to_non_nullable
as U8Array32,
  ));
}

}



/// @nodoc


class _UiMimiId implements UiMimiId {
  const _UiMimiId({required this.field0});
  

@override final  U8Array32 field0;

/// Create a copy of UiMimiId
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
_$UiMimiIdCopyWith<_UiMimiId> get copyWith => __$UiMimiIdCopyWithImpl<_UiMimiId>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is _UiMimiId&&const DeepCollectionEquality().equals(other.field0, field0));
}


@override
int get hashCode => Object.hash(runtimeType,const DeepCollectionEquality().hash(field0));

@override
String toString() {
  return 'UiMimiId(field0: $field0)';
}


}

/// @nodoc
abstract mixin class _$UiMimiIdCopyWith<$Res> implements $UiMimiIdCopyWith<$Res> {
  factory _$UiMimiIdCopyWith(_UiMimiId value, $Res Function(_UiMimiId) _then) = __$UiMimiIdCopyWithImpl;
@override @useResult
$Res call({
 U8Array32 field0
});




}
/// @nodoc
class __$UiMimiIdCopyWithImpl<$Res>
    implements _$UiMimiIdCopyWith<$Res> {
  __$UiMimiIdCopyWithImpl(this._self, this._then);

  final _UiMimiId _self;
  final $Res Function(_UiMimiId) _then;

/// Create a copy of UiMimiId
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? field0 = null,}) {
  return _then(_UiMimiId(
field0: null == field0 ? _self.field0 : field0 // ignore: cast_nullable_to_non_nullable
as U8Array32,
  ));
}


}

// dart format on
