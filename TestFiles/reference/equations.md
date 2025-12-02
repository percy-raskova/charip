# Equations

MyST supports LaTeX math with labeled equations.

## Inline Math

The quadratic formula is $x = \frac{-b \pm \sqrt{b^2-4ac}}{2a}$.

## Display Math

```{math}
:label: euler-identity

e^{i\pi} + 1 = 0
```

Reference this equation with {eq}`euler-identity`.

```{math}
:label: pythagorean

a^2 + b^2 = c^2
```

The {eq}`pythagorean` theorem is fundamental to geometry.

## Figures with Numbers

```{figure} ../assets/example.zip
:name: figure-example

Example figure for numref testing.
```

Reference with {numref}`figure-example`.

## Related

- {doc}`api` for LSP methods
- {ref}`api-docs` for overview
- {term}`MyST` syntax

#math #equations #latex
