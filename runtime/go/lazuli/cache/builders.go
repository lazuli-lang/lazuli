package cache

import "time"

// Of starts a fluent QuerySpec builder.
//
//	spec := cache.Of("customers").TTL(5 * time.Minute).Tags("billing", "tenant").Namespace("v1").Build()
func Of(key string) SpecBuilder {
	return SpecBuilder{key: key}
}

// SpecBuilder builds QuerySpec values with a fluent API.
type SpecBuilder struct {
	key       string
	ttl       time.Duration
	tags      []string
	namespace string
}

// TTL sets the cache entry lifetime.
func (b SpecBuilder) TTL(d time.Duration) SpecBuilder {
	b.ttl = d
	return b
}

// Tags appends cache invalidation tags.
func (b SpecBuilder) Tags(tags ...string) SpecBuilder {
	b.tags = append(b.tags, tags...)
	return b
}

// Namespace sets an additional cache key namespace.
func (b SpecBuilder) Namespace(ns string) SpecBuilder {
	b.namespace = ns
	return b
}

// Build returns the completed QuerySpec.
func (b SpecBuilder) Build() QuerySpec {
	return QuerySpec{
		Key:       b.key,
		TTL:       b.ttl,
		Tags:      b.tags,
		Namespace: b.namespace,
	}
}
