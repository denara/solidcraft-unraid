//! A rectangular net of values: a surface's control points, a sample grid. Rectangular by
//! construction, so code that walks it never meets a missing cell: a grid is made from a
//! function of `(i, j)`, from one row, or from rows checked once ([`Grid::from_u_rows`]).

/// `count[0]` values along u by `count[1]` along v; value `(i, j)` (i along u) is stored at
/// `j·count[0] + i`, u fastest.
#[derive(Clone, Debug, PartialEq)]
pub struct Grid<T> {
    count: [usize; 2],
    cells: Vec<T>,
}

impl<T> Grid<T> {
    /// A grid of `f(i, j)` for every cell; `None` when `count[0]·count[1]` overflows.
    pub fn from_fn(count: [usize; 2], mut f: impl FnMut(usize, usize) -> T) -> Option<Grid<T>> {
        let total = count[0].checked_mul(count[1])?;
        let cu = count[0].max(1);
        Some(Grid { count, cells: (0..total).map(|k| f(k % cu, k / cu)).collect() })
    }

    /// One row of values along u: `count` is `[cells.len(), 1]` (a curve's control points seen
    /// as a net).
    pub fn row(cells: Vec<T>) -> Grid<T> {
        Grid { count: [cells.len(), 1], cells }
    }

    /// A grid whose caller already holds `count[0]·count[1]` cells (a checked surface's net).
    pub(crate) fn checked(count: [usize; 2], cells: Vec<T>) -> Grid<T> {
        Grid { count, cells }
    }

    /// Values along u and along v.
    pub fn count(&self) -> [usize; 2] {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    /// Value `(i, j)`, i along u.
    pub fn get(&self, i: usize, j: usize) -> Option<&T> {
        if i < self.count[0] { self.cells.get(j * self.count[0] + i) } else { None }
    }

    /// The values, u fastest.
    pub fn cells(&self) -> &[T] {
        &self.cells
    }

    /// The runs along u, one per v index: run `j` holds values `(0, j)`, `(1, j)`, ….
    pub fn lines(&self) -> std::slice::Chunks<'_, T> {
        self.cells.chunks(self.count[0].max(1))
    }

    /// The grid with u and v swapped: value `(i, j)` becomes value `(j, i)`.
    pub fn transposed(&self) -> Grid<T>
    where
        T: Clone,
    {
        let [cu, cv] = self.count;
        let cells = (0..self.cells.len()).filter_map(|k| self.get(k / cv.max(1), k % cv.max(1)).cloned()).collect();
        Grid { count: [cv, cu], cells }
    }

    /// The values to change in place; the grid keeps its shape.
    pub fn cells_mut(&mut self) -> &mut [T] {
        &mut self.cells
    }

    pub fn into_cells(self) -> Vec<T> {
        self.cells
    }

    pub fn map<U>(self, f: impl FnMut(T) -> U) -> Grid<U> {
        Grid { count: self.count, cells: self.cells.into_iter().map(f).collect() }
    }

    /// [`Grid::map`] with a step that can fail: the first failure.
    pub fn try_map<U, E>(self, f: impl FnMut(T) -> Result<U, E>) -> Result<Grid<U>, E> {
        Ok(Grid { count: self.count, cells: self.cells.into_iter().map(f).collect::<Result<_, _>>()? })
    }

    /// The grid as rows, one per u index: `rows[i][j]` is value `(i, j)` (truck's layout).
    pub fn into_u_rows(self) -> Vec<Vec<T>> {
        let cu = self.count[0];
        let mut rows: Vec<Vec<T>> = (0..cu).map(|_| Vec::with_capacity(self.count[1])).collect();
        for (k, v) in self.cells.into_iter().enumerate() {
            if let Some(row) = rows.get_mut(k % cu) {
                row.push(v);
            }
        }
        rows
    }
}

impl<T: Clone> Grid<T> {
    /// A grid from rows, one per u index: `rows[i][j]` is value `(i, j)` (truck's layout).
    /// `None` when the rows differ in length, the one check rows from elsewhere need.
    pub fn from_u_rows<R: AsRef<[T]>>(rows: &[R]) -> Option<Grid<T>> {
        let cv = rows.first().map_or(0, |r| r.as_ref().len());
        if rows.iter().any(|r| r.as_ref().len() != cv) {
            return None;
        }
        let cells = (0..cv).flat_map(|j| rows.iter().filter_map(move |r| r.as_ref().get(j).cloned())).collect();
        Some(Grid { count: [rows.len(), cv], cells })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_go_in_and_come_back_out() {
        // rows[i][j]: two along u, three along v.
        let rows = vec![vec![(0, 0), (0, 1), (0, 2)], vec![(1, 0), (1, 1), (1, 2)]];
        let g = Grid::from_u_rows(&rows).unwrap();
        assert_eq!(g.count(), [2, 3]);
        assert_eq!(g.cells(), &[(0, 0), (1, 0), (0, 1), (1, 1), (0, 2), (1, 2)]);
        assert_eq!(g.get(1, 2), Some(&(1, 2)));
        assert_eq!(g.get(2, 0), None);
        assert_eq!(g.into_u_rows(), rows);
        assert_eq!(Grid::row(vec![1, 2, 3]).count(), [3, 1]);
        let g = Grid::from_fn([3, 2], |i, j| (i, j)).unwrap();
        assert_eq!(g.lines().collect::<Vec<_>>(), vec![&[(0, 0), (1, 0), (2, 0)][..], &[(0, 1), (1, 1), (2, 1)][..]]);
        let t = g.transposed();
        assert_eq!(t.count(), [2, 3]);
        assert!((0..3).all(|i| (0..2).all(|j| t.get(j, i) == g.get(i, j))));
        assert!(Grid::from_fn([usize::MAX, 2], |_, _| 0).is_none());
    }

    #[test]
    fn only_rectangles_are_grids() {
        assert!(Grid::from_u_rows(&[vec![1, 2], vec![3]]).is_none());
        let empty: [Vec<u8>; 0] = [];
        assert!(Grid::from_u_rows(&empty).is_some_and(|g| g.is_empty()));
    }

    #[test]
    fn try_map_stops_at_the_first_failure() {
        assert_eq!(Grid::row(vec![1, -1, -2]).try_map(|x| if x > 0 { Ok(x) } else { Err(x) }), Err(-1));
    }
}
