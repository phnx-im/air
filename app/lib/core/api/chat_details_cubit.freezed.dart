// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'chat_details_cubit.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// dart format off
T _$identity<T>(T value) => value;
/// @nodoc
mixin _$ChatDetailsState {

 UiChatDetails? get chat; List<UiUserId> get members;
/// Create a copy of ChatDetailsState
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$ChatDetailsStateCopyWith<ChatDetailsState> get copyWith => _$ChatDetailsStateCopyWithImpl<ChatDetailsState>(this as ChatDetailsState, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is ChatDetailsState&&(identical(other.chat, chat) || other.chat == chat)&&const DeepCollectionEquality().equals(other.members, members));
}


@override
int get hashCode => Object.hash(runtimeType,chat,const DeepCollectionEquality().hash(members));

@override
String toString() {
  return 'ChatDetailsState(chat: $chat, members: $members)';
}


}

/// @nodoc
abstract mixin class $ChatDetailsStateCopyWith<$Res>  {
  factory $ChatDetailsStateCopyWith(ChatDetailsState value, $Res Function(ChatDetailsState) _then) = _$ChatDetailsStateCopyWithImpl;
@useResult
$Res call({
 UiChatDetails? chat, List<UiUserId> members
});




}
/// @nodoc
class _$ChatDetailsStateCopyWithImpl<$Res>
    implements $ChatDetailsStateCopyWith<$Res> {
  _$ChatDetailsStateCopyWithImpl(this._self, this._then);

  final ChatDetailsState _self;
  final $Res Function(ChatDetailsState) _then;

/// Create a copy of ChatDetailsState
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') @override $Res call({Object? chat = freezed,Object? members = null,}) {
  return _then(_self.copyWith(
chat: freezed == chat ? _self.chat : chat // ignore: cast_nullable_to_non_nullable
as UiChatDetails?,members: null == members ? _self.members : members // ignore: cast_nullable_to_non_nullable
as List<UiUserId>,
  ));
}

}



/// @nodoc


class _ChatDetailsState extends ChatDetailsState {
  const _ChatDetailsState({this.chat, required final  List<UiUserId> members}): _members = members,super._();
  

@override final  UiChatDetails? chat;
 final  List<UiUserId> _members;
@override List<UiUserId> get members {
  if (_members is EqualUnmodifiableListView) return _members;
  // ignore: implicit_dynamic_type
  return EqualUnmodifiableListView(_members);
}


/// Create a copy of ChatDetailsState
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
_$ChatDetailsStateCopyWith<_ChatDetailsState> get copyWith => __$ChatDetailsStateCopyWithImpl<_ChatDetailsState>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is _ChatDetailsState&&(identical(other.chat, chat) || other.chat == chat)&&const DeepCollectionEquality().equals(other._members, _members));
}


@override
int get hashCode => Object.hash(runtimeType,chat,const DeepCollectionEquality().hash(_members));

@override
String toString() {
  return 'ChatDetailsState(chat: $chat, members: $members)';
}


}

/// @nodoc
abstract mixin class _$ChatDetailsStateCopyWith<$Res> implements $ChatDetailsStateCopyWith<$Res> {
  factory _$ChatDetailsStateCopyWith(_ChatDetailsState value, $Res Function(_ChatDetailsState) _then) = __$ChatDetailsStateCopyWithImpl;
@override @useResult
$Res call({
 UiChatDetails? chat, List<UiUserId> members
});




}
/// @nodoc
class __$ChatDetailsStateCopyWithImpl<$Res>
    implements _$ChatDetailsStateCopyWith<$Res> {
  __$ChatDetailsStateCopyWithImpl(this._self, this._then);

  final _ChatDetailsState _self;
  final $Res Function(_ChatDetailsState) _then;

/// Create a copy of ChatDetailsState
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? chat = freezed,Object? members = null,}) {
  return _then(_ChatDetailsState(
chat: freezed == chat ? _self.chat : chat // ignore: cast_nullable_to_non_nullable
as UiChatDetails?,members: null == members ? _self._members : members // ignore: cast_nullable_to_non_nullable
as List<UiUserId>,
  ));
}


}

/// @nodoc
mixin _$UploadAttachmentError {

 BigInt get maxSizeBytes; BigInt get actualSizeBytes;
/// Create a copy of UploadAttachmentError
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$UploadAttachmentErrorCopyWith<UploadAttachmentError> get copyWith => _$UploadAttachmentErrorCopyWithImpl<UploadAttachmentError>(this as UploadAttachmentError, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is UploadAttachmentError&&(identical(other.maxSizeBytes, maxSizeBytes) || other.maxSizeBytes == maxSizeBytes)&&(identical(other.actualSizeBytes, actualSizeBytes) || other.actualSizeBytes == actualSizeBytes));
}


@override
int get hashCode => Object.hash(runtimeType,maxSizeBytes,actualSizeBytes);

@override
String toString() {
  return 'UploadAttachmentError(maxSizeBytes: $maxSizeBytes, actualSizeBytes: $actualSizeBytes)';
}


}

/// @nodoc
abstract mixin class $UploadAttachmentErrorCopyWith<$Res>  {
  factory $UploadAttachmentErrorCopyWith(UploadAttachmentError value, $Res Function(UploadAttachmentError) _then) = _$UploadAttachmentErrorCopyWithImpl;
@useResult
$Res call({
 BigInt maxSizeBytes, BigInt actualSizeBytes
});




}
/// @nodoc
class _$UploadAttachmentErrorCopyWithImpl<$Res>
    implements $UploadAttachmentErrorCopyWith<$Res> {
  _$UploadAttachmentErrorCopyWithImpl(this._self, this._then);

  final UploadAttachmentError _self;
  final $Res Function(UploadAttachmentError) _then;

/// Create a copy of UploadAttachmentError
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') @override $Res call({Object? maxSizeBytes = null,Object? actualSizeBytes = null,}) {
  return _then(_self.copyWith(
maxSizeBytes: null == maxSizeBytes ? _self.maxSizeBytes : maxSizeBytes // ignore: cast_nullable_to_non_nullable
as BigInt,actualSizeBytes: null == actualSizeBytes ? _self.actualSizeBytes : actualSizeBytes // ignore: cast_nullable_to_non_nullable
as BigInt,
  ));
}

}



/// @nodoc


class UploadAttachmentError_TooLarge extends UploadAttachmentError {
  const UploadAttachmentError_TooLarge({required this.maxSizeBytes, required this.actualSizeBytes}): super._();
  

@override final  BigInt maxSizeBytes;
@override final  BigInt actualSizeBytes;

/// Create a copy of UploadAttachmentError
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$UploadAttachmentError_TooLargeCopyWith<UploadAttachmentError_TooLarge> get copyWith => _$UploadAttachmentError_TooLargeCopyWithImpl<UploadAttachmentError_TooLarge>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is UploadAttachmentError_TooLarge&&(identical(other.maxSizeBytes, maxSizeBytes) || other.maxSizeBytes == maxSizeBytes)&&(identical(other.actualSizeBytes, actualSizeBytes) || other.actualSizeBytes == actualSizeBytes));
}


@override
int get hashCode => Object.hash(runtimeType,maxSizeBytes,actualSizeBytes);

@override
String toString() {
  return 'UploadAttachmentError.tooLarge(maxSizeBytes: $maxSizeBytes, actualSizeBytes: $actualSizeBytes)';
}


}

/// @nodoc
abstract mixin class $UploadAttachmentError_TooLargeCopyWith<$Res> implements $UploadAttachmentErrorCopyWith<$Res> {
  factory $UploadAttachmentError_TooLargeCopyWith(UploadAttachmentError_TooLarge value, $Res Function(UploadAttachmentError_TooLarge) _then) = _$UploadAttachmentError_TooLargeCopyWithImpl;
@override @useResult
$Res call({
 BigInt maxSizeBytes, BigInt actualSizeBytes
});




}
/// @nodoc
class _$UploadAttachmentError_TooLargeCopyWithImpl<$Res>
    implements $UploadAttachmentError_TooLargeCopyWith<$Res> {
  _$UploadAttachmentError_TooLargeCopyWithImpl(this._self, this._then);

  final UploadAttachmentError_TooLarge _self;
  final $Res Function(UploadAttachmentError_TooLarge) _then;

/// Create a copy of UploadAttachmentError
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? maxSizeBytes = null,Object? actualSizeBytes = null,}) {
  return _then(UploadAttachmentError_TooLarge(
maxSizeBytes: null == maxSizeBytes ? _self.maxSizeBytes : maxSizeBytes // ignore: cast_nullable_to_non_nullable
as BigInt,actualSizeBytes: null == actualSizeBytes ? _self.actualSizeBytes : actualSizeBytes // ignore: cast_nullable_to_non_nullable
as BigInt,
  ));
}


}

// dart format on
