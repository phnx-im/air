// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'message_cubit.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// dart format off
T _$identity<T>(T value) => value;
/// @nodoc
mixin _$MessageState {

 UiChatMessage get message;
/// Create a copy of MessageState
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$MessageStateCopyWith<MessageState> get copyWith => _$MessageStateCopyWithImpl<MessageState>(this as MessageState, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is MessageState&&(identical(other.message, message) || other.message == message));
}


@override
int get hashCode => Object.hash(runtimeType,message);

@override
String toString() {
  return 'MessageState(message: $message)';
}


}

/// @nodoc
abstract mixin class $MessageStateCopyWith<$Res>  {
  factory $MessageStateCopyWith(MessageState value, $Res Function(MessageState) _then) = _$MessageStateCopyWithImpl;
@useResult
$Res call({
 UiChatMessage message
});


$UiChatMessageCopyWith<$Res> get message;

}
/// @nodoc
class _$MessageStateCopyWithImpl<$Res>
    implements $MessageStateCopyWith<$Res> {
  _$MessageStateCopyWithImpl(this._self, this._then);

  final MessageState _self;
  final $Res Function(MessageState) _then;

/// Create a copy of MessageState
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') @override $Res call({Object? message = null,}) {
  return _then(_self.copyWith(
message: null == message ? _self.message : message // ignore: cast_nullable_to_non_nullable
as UiChatMessage,
  ));
}
/// Create a copy of MessageState
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$UiChatMessageCopyWith<$Res> get message {
  
  return $UiChatMessageCopyWith<$Res>(_self.message, (value) {
    return _then(_self.copyWith(message: value));
  });
}
}



/// @nodoc


class _MessageState implements MessageState {
  const _MessageState({required this.message});
  

@override final  UiChatMessage message;

/// Create a copy of MessageState
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
_$MessageStateCopyWith<_MessageState> get copyWith => __$MessageStateCopyWithImpl<_MessageState>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is _MessageState&&(identical(other.message, message) || other.message == message));
}


@override
int get hashCode => Object.hash(runtimeType,message);

@override
String toString() {
  return 'MessageState(message: $message)';
}


}

/// @nodoc
abstract mixin class _$MessageStateCopyWith<$Res> implements $MessageStateCopyWith<$Res> {
  factory _$MessageStateCopyWith(_MessageState value, $Res Function(_MessageState) _then) = __$MessageStateCopyWithImpl;
@override @useResult
$Res call({
 UiChatMessage message
});


@override $UiChatMessageCopyWith<$Res> get message;

}
/// @nodoc
class __$MessageStateCopyWithImpl<$Res>
    implements _$MessageStateCopyWith<$Res> {
  __$MessageStateCopyWithImpl(this._self, this._then);

  final _MessageState _self;
  final $Res Function(_MessageState) _then;

/// Create a copy of MessageState
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? message = null,}) {
  return _then(_MessageState(
message: null == message ? _self.message : message // ignore: cast_nullable_to_non_nullable
as UiChatMessage,
  ));
}

/// Create a copy of MessageState
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$UiChatMessageCopyWith<$Res> get message {
  
  return $UiChatMessageCopyWith<$Res>(_self.message, (value) {
    return _then(_self.copyWith(message: value));
  });
}
}

// dart format on
