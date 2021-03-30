// Copyright 2021 The Fuchsia Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

package fidlgen_cpp

import (
	"fmt"

	"go.fuchsia.dev/fuchsia/tools/fidl/lib/fidlgen"
)

type Union struct {
	fidlgen.Attributes
	fidlgen.Strictness
	fidlgen.Resourceness
	NameVariants
	CodingTableType    string
	TagEnum            NameVariants
	TagUnknown         NameVariants
	TagInvalid         NameVariants
	WireOrdinalEnum    Name
	WireInvalidOrdinal Name
	Members            []UnionMember
	InlineSize         int
	MaxHandles         int
	MaxOutOfLine       int
	Result             *Result
	HasPointer         bool
}

func (Union) Kind() declKind {
	return Kinds.Union
}

var _ Kinded = (*Union)(nil)

type UnionMember struct {
	fidlgen.Attributes
	Ordinal           uint64
	Type              Type
	Name              string
	StorageName       string
	TagName           NameVariants
	WireOrdinalName   Name
	Offset            int
	HandleInformation *HandleInformation
}

func (um UnionMember) UpperCamelCaseName() string {
	return fidlgen.ToUpperCamelCase(um.Name)
}

func (um UnionMember) NameAndType() (string, Type) {
	return um.Name, um.Type
}
func (c *compiler) compileUnion(val fidlgen.Union) Union {
	name := c.compileNameVariants(val.Name)
	codingTableType := c.compileCodingTableType(val.Name)
	tagEnum := name.Nest("Tag")
	wireOrdinalEnum := name.Wire.Nest("Ordinal")
	u := Union{
		Attributes:         val.Attributes,
		Strictness:         val.Strictness,
		Resourceness:       val.Resourceness,
		NameVariants:       name,
		CodingTableType:    codingTableType,
		TagEnum:            tagEnum,
		TagUnknown:         tagEnum.Nest("kUnknown"),
		TagInvalid:         tagEnum.Nest("Invalid"),
		WireOrdinalEnum:    wireOrdinalEnum,
		WireInvalidOrdinal: wireOrdinalEnum.Nest("Invalid"),
		InlineSize:         val.TypeShapeV1.InlineSize,
		MaxHandles:         val.TypeShapeV1.MaxHandles,
		MaxOutOfLine:       val.TypeShapeV1.MaxOutOfLine,
		HasPointer:         val.TypeShapeV1.Depth > 0,
	}

	for _, mem := range val.Members {
		if mem.Reserved {
			continue
		}
		n := changeIfReserved(mem.Name)
		t := fmt.Sprintf("k%s", fidlgen.ToUpperCamelCase(n))
		u.Members = append(u.Members, UnionMember{
			Attributes:        mem.Attributes,
			Ordinal:           uint64(mem.Ordinal),
			Type:              c.compileType(mem.Type),
			Name:              n,
			StorageName:       changeIfReserved(mem.Name + "_"),
			TagName:           u.TagEnum.Nest(t),
			WireOrdinalName:   u.WireOrdinalEnum.Nest(t),
			Offset:            mem.Offset,
			HandleInformation: c.fieldHandleInformation(&mem.Type),
		})
	}

	if val.MethodResult != nil {
		result := Result{
			ResultDecl:      u.NameVariants,
			ValueStructDecl: u.Members[0].Type.NameVariants,
			ErrorDecl:       u.Members[1].Type.NameVariants,
		}
		c.resultForStruct[val.MethodResult.ValueType.Identifier] = &result
		c.resultForUnion[val.Name] = &result
		u.Result = &result
	}

	return u
}
