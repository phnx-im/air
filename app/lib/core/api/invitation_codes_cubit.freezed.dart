// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'invitation_codes_cubit.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// dart format off
T _$identity<T>(T value) => value;
/// @nodoc
mixin _$InvitationCode {

 String get code; bool get copied; DateTime get createdAt;
/// Create a copy of InvitationCode
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$InvitationCodeCopyWith<InvitationCode> get copyWith => _$InvitationCodeCopyWithImpl<InvitationCode>(this as InvitationCode, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is InvitationCode&&(identical(other.code, code) || other.code == code)&&(identical(other.copied, copied) || other.copied == copied)&&(identical(other.createdAt, createdAt) || other.createdAt == createdAt));
}


@override
int get hashCode => Object.hash(runtimeType,code,copied,createdAt);

@override
String toString() {
  return 'InvitationCode(code: $code, copied: $copied, createdAt: $createdAt)';
}


}

/// @nodoc
abstract mixin class $InvitationCodeCopyWith<$Res>  {
  factory $InvitationCodeCopyWith(InvitationCode value, $Res Function(InvitationCode) _then) = _$InvitationCodeCopyWithImpl;
@useResult
$Res call({
 String code, bool copied, DateTime createdAt
});




}
/// @nodoc
class _$InvitationCodeCopyWithImpl<$Res>
    implements $InvitationCodeCopyWith<$Res> {
  _$InvitationCodeCopyWithImpl(this._self, this._then);

  final InvitationCode _self;
  final $Res Function(InvitationCode) _then;

/// Create a copy of InvitationCode
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') @override $Res call({Object? code = null,Object? copied = null,Object? createdAt = null,}) {
  return _then(_self.copyWith(
code: null == code ? _self.code : code // ignore: cast_nullable_to_non_nullable
as String,copied: null == copied ? _self.copied : copied // ignore: cast_nullable_to_non_nullable
as bool,createdAt: null == createdAt ? _self.createdAt : createdAt // ignore: cast_nullable_to_non_nullable
as DateTime,
  ));
}

}



/// @nodoc


class _InvitationCode implements InvitationCode {
  const _InvitationCode({required this.code, required this.copied, required this.createdAt});
  

@override final  String code;
@override final  bool copied;
@override final  DateTime createdAt;

/// Create a copy of InvitationCode
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
_$InvitationCodeCopyWith<_InvitationCode> get copyWith => __$InvitationCodeCopyWithImpl<_InvitationCode>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is _InvitationCode&&(identical(other.code, code) || other.code == code)&&(identical(other.copied, copied) || other.copied == copied)&&(identical(other.createdAt, createdAt) || other.createdAt == createdAt));
}


@override
int get hashCode => Object.hash(runtimeType,code,copied,createdAt);

@override
String toString() {
  return 'InvitationCode(code: $code, copied: $copied, createdAt: $createdAt)';
}


}

/// @nodoc
abstract mixin class _$InvitationCodeCopyWith<$Res> implements $InvitationCodeCopyWith<$Res> {
  factory _$InvitationCodeCopyWith(_InvitationCode value, $Res Function(_InvitationCode) _then) = __$InvitationCodeCopyWithImpl;
@override @useResult
$Res call({
 String code, bool copied, DateTime createdAt
});




}
/// @nodoc
class __$InvitationCodeCopyWithImpl<$Res>
    implements _$InvitationCodeCopyWith<$Res> {
  __$InvitationCodeCopyWithImpl(this._self, this._then);

  final _InvitationCode _self;
  final $Res Function(_InvitationCode) _then;

/// Create a copy of InvitationCode
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? code = null,Object? copied = null,Object? createdAt = null,}) {
  return _then(_InvitationCode(
code: null == code ? _self.code : code // ignore: cast_nullable_to_non_nullable
as String,copied: null == copied ? _self.copied : copied // ignore: cast_nullable_to_non_nullable
as bool,createdAt: null == createdAt ? _self.createdAt : createdAt // ignore: cast_nullable_to_non_nullable
as DateTime,
  ));
}


}

/// @nodoc
mixin _$InvitationCodesState {

 List<UiInvitationCode> get codes;
/// Create a copy of InvitationCodesState
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$InvitationCodesStateCopyWith<InvitationCodesState> get copyWith => _$InvitationCodesStateCopyWithImpl<InvitationCodesState>(this as InvitationCodesState, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is InvitationCodesState&&const DeepCollectionEquality().equals(other.codes, codes));
}


@override
int get hashCode => Object.hash(runtimeType,const DeepCollectionEquality().hash(codes));

@override
String toString() {
  return 'InvitationCodesState(codes: $codes)';
}


}

/// @nodoc
abstract mixin class $InvitationCodesStateCopyWith<$Res>  {
  factory $InvitationCodesStateCopyWith(InvitationCodesState value, $Res Function(InvitationCodesState) _then) = _$InvitationCodesStateCopyWithImpl;
@useResult
$Res call({
 List<UiInvitationCode> codes
});




}
/// @nodoc
class _$InvitationCodesStateCopyWithImpl<$Res>
    implements $InvitationCodesStateCopyWith<$Res> {
  _$InvitationCodesStateCopyWithImpl(this._self, this._then);

  final InvitationCodesState _self;
  final $Res Function(InvitationCodesState) _then;

/// Create a copy of InvitationCodesState
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') @override $Res call({Object? codes = null,}) {
  return _then(_self.copyWith(
codes: null == codes ? _self.codes : codes // ignore: cast_nullable_to_non_nullable
as List<UiInvitationCode>,
  ));
}

}



/// @nodoc


class _InvitationCodesState extends InvitationCodesState {
  const _InvitationCodesState({required final  List<UiInvitationCode> codes}): _codes = codes,super._();
  

 final  List<UiInvitationCode> _codes;
@override List<UiInvitationCode> get codes {
  if (_codes is EqualUnmodifiableListView) return _codes;
  // ignore: implicit_dynamic_type
  return EqualUnmodifiableListView(_codes);
}


/// Create a copy of InvitationCodesState
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
_$InvitationCodesStateCopyWith<_InvitationCodesState> get copyWith => __$InvitationCodesStateCopyWithImpl<_InvitationCodesState>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is _InvitationCodesState&&const DeepCollectionEquality().equals(other._codes, _codes));
}


@override
int get hashCode => Object.hash(runtimeType,const DeepCollectionEquality().hash(_codes));

@override
String toString() {
  return 'InvitationCodesState(codes: $codes)';
}


}

/// @nodoc
abstract mixin class _$InvitationCodesStateCopyWith<$Res> implements $InvitationCodesStateCopyWith<$Res> {
  factory _$InvitationCodesStateCopyWith(_InvitationCodesState value, $Res Function(_InvitationCodesState) _then) = __$InvitationCodesStateCopyWithImpl;
@override @useResult
$Res call({
 List<UiInvitationCode> codes
});




}
/// @nodoc
class __$InvitationCodesStateCopyWithImpl<$Res>
    implements _$InvitationCodesStateCopyWith<$Res> {
  __$InvitationCodesStateCopyWithImpl(this._self, this._then);

  final _InvitationCodesState _self;
  final $Res Function(_InvitationCodesState) _then;

/// Create a copy of InvitationCodesState
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? codes = null,}) {
  return _then(_InvitationCodesState(
codes: null == codes ? _self._codes : codes // ignore: cast_nullable_to_non_nullable
as List<UiInvitationCode>,
  ));
}


}

/// @nodoc
mixin _$TokenId {

 PlatformInt64 get id; DateTime get createdAt;
/// Create a copy of TokenId
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$TokenIdCopyWith<TokenId> get copyWith => _$TokenIdCopyWithImpl<TokenId>(this as TokenId, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is TokenId&&(identical(other.id, id) || other.id == id)&&(identical(other.createdAt, createdAt) || other.createdAt == createdAt));
}


@override
int get hashCode => Object.hash(runtimeType,id,createdAt);

@override
String toString() {
  return 'TokenId(id: $id, createdAt: $createdAt)';
}


}

/// @nodoc
abstract mixin class $TokenIdCopyWith<$Res>  {
  factory $TokenIdCopyWith(TokenId value, $Res Function(TokenId) _then) = _$TokenIdCopyWithImpl;
@useResult
$Res call({
 PlatformInt64 id, DateTime createdAt
});




}
/// @nodoc
class _$TokenIdCopyWithImpl<$Res>
    implements $TokenIdCopyWith<$Res> {
  _$TokenIdCopyWithImpl(this._self, this._then);

  final TokenId _self;
  final $Res Function(TokenId) _then;

/// Create a copy of TokenId
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') @override $Res call({Object? id = null,Object? createdAt = null,}) {
  return _then(_self.copyWith(
id: null == id ? _self.id : id // ignore: cast_nullable_to_non_nullable
as PlatformInt64,createdAt: null == createdAt ? _self.createdAt : createdAt // ignore: cast_nullable_to_non_nullable
as DateTime,
  ));
}

}



/// @nodoc


class _TokenId implements TokenId {
  const _TokenId({required this.id, required this.createdAt});
  

@override final  PlatformInt64 id;
@override final  DateTime createdAt;

/// Create a copy of TokenId
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
_$TokenIdCopyWith<_TokenId> get copyWith => __$TokenIdCopyWithImpl<_TokenId>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is _TokenId&&(identical(other.id, id) || other.id == id)&&(identical(other.createdAt, createdAt) || other.createdAt == createdAt));
}


@override
int get hashCode => Object.hash(runtimeType,id,createdAt);

@override
String toString() {
  return 'TokenId(id: $id, createdAt: $createdAt)';
}


}

/// @nodoc
abstract mixin class _$TokenIdCopyWith<$Res> implements $TokenIdCopyWith<$Res> {
  factory _$TokenIdCopyWith(_TokenId value, $Res Function(_TokenId) _then) = __$TokenIdCopyWithImpl;
@override @useResult
$Res call({
 PlatformInt64 id, DateTime createdAt
});




}
/// @nodoc
class __$TokenIdCopyWithImpl<$Res>
    implements _$TokenIdCopyWith<$Res> {
  __$TokenIdCopyWithImpl(this._self, this._then);

  final _TokenId _self;
  final $Res Function(_TokenId) _then;

/// Create a copy of TokenId
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? id = null,Object? createdAt = null,}) {
  return _then(_TokenId(
id: null == id ? _self.id : id // ignore: cast_nullable_to_non_nullable
as PlatformInt64,createdAt: null == createdAt ? _self.createdAt : createdAt // ignore: cast_nullable_to_non_nullable
as DateTime,
  ));
}


}

/// @nodoc
mixin _$UiInvitationCode {

 Object get field0;



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is UiInvitationCode&&const DeepCollectionEquality().equals(other.field0, field0));
}


@override
int get hashCode => Object.hash(runtimeType,const DeepCollectionEquality().hash(field0));

@override
String toString() {
  return 'UiInvitationCode(field0: $field0)';
}


}

/// @nodoc
class $UiInvitationCodeCopyWith<$Res>  {
$UiInvitationCodeCopyWith(UiInvitationCode _, $Res Function(UiInvitationCode) __);
}



/// @nodoc


class UiInvitationCode_Token extends UiInvitationCode {
  const UiInvitationCode_Token(this.field0): super._();
  

@override final  TokenId field0;

/// Create a copy of UiInvitationCode
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$UiInvitationCode_TokenCopyWith<UiInvitationCode_Token> get copyWith => _$UiInvitationCode_TokenCopyWithImpl<UiInvitationCode_Token>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is UiInvitationCode_Token&&(identical(other.field0, field0) || other.field0 == field0));
}


@override
int get hashCode => Object.hash(runtimeType,field0);

@override
String toString() {
  return 'UiInvitationCode.token(field0: $field0)';
}


}

/// @nodoc
abstract mixin class $UiInvitationCode_TokenCopyWith<$Res> implements $UiInvitationCodeCopyWith<$Res> {
  factory $UiInvitationCode_TokenCopyWith(UiInvitationCode_Token value, $Res Function(UiInvitationCode_Token) _then) = _$UiInvitationCode_TokenCopyWithImpl;
@useResult
$Res call({
 TokenId field0
});


$TokenIdCopyWith<$Res> get field0;

}
/// @nodoc
class _$UiInvitationCode_TokenCopyWithImpl<$Res>
    implements $UiInvitationCode_TokenCopyWith<$Res> {
  _$UiInvitationCode_TokenCopyWithImpl(this._self, this._then);

  final UiInvitationCode_Token _self;
  final $Res Function(UiInvitationCode_Token) _then;

/// Create a copy of UiInvitationCode
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? field0 = null,}) {
  return _then(UiInvitationCode_Token(
null == field0 ? _self.field0 : field0 // ignore: cast_nullable_to_non_nullable
as TokenId,
  ));
}

/// Create a copy of UiInvitationCode
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$TokenIdCopyWith<$Res> get field0 {
  
  return $TokenIdCopyWith<$Res>(_self.field0, (value) {
    return _then(_self.copyWith(field0: value));
  });
}
}

/// @nodoc


class UiInvitationCode_Code extends UiInvitationCode {
  const UiInvitationCode_Code(this.field0): super._();
  

@override final  InvitationCode field0;

/// Create a copy of UiInvitationCode
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$UiInvitationCode_CodeCopyWith<UiInvitationCode_Code> get copyWith => _$UiInvitationCode_CodeCopyWithImpl<UiInvitationCode_Code>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is UiInvitationCode_Code&&(identical(other.field0, field0) || other.field0 == field0));
}


@override
int get hashCode => Object.hash(runtimeType,field0);

@override
String toString() {
  return 'UiInvitationCode.code(field0: $field0)';
}


}

/// @nodoc
abstract mixin class $UiInvitationCode_CodeCopyWith<$Res> implements $UiInvitationCodeCopyWith<$Res> {
  factory $UiInvitationCode_CodeCopyWith(UiInvitationCode_Code value, $Res Function(UiInvitationCode_Code) _then) = _$UiInvitationCode_CodeCopyWithImpl;
@useResult
$Res call({
 InvitationCode field0
});


$InvitationCodeCopyWith<$Res> get field0;

}
/// @nodoc
class _$UiInvitationCode_CodeCopyWithImpl<$Res>
    implements $UiInvitationCode_CodeCopyWith<$Res> {
  _$UiInvitationCode_CodeCopyWithImpl(this._self, this._then);

  final UiInvitationCode_Code _self;
  final $Res Function(UiInvitationCode_Code) _then;

/// Create a copy of UiInvitationCode
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? field0 = null,}) {
  return _then(UiInvitationCode_Code(
null == field0 ? _self.field0 : field0 // ignore: cast_nullable_to_non_nullable
as InvitationCode,
  ));
}

/// Create a copy of UiInvitationCode
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$InvitationCodeCopyWith<$Res> get field0 {
  
  return $InvitationCodeCopyWith<$Res>(_self.field0, (value) {
    return _then(_self.copyWith(field0: value));
  });
}
}

// dart format on
