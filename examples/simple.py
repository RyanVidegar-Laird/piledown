from pathlib import Path
from pyledown import PileParams, Strand, LibFragmentType
import pandas as pd
import pyarrow as pa
import seaborn as sns
import matplotlib.pyplot as plt

input_bam = Path() / "tests" / "data" / "SRR21778056-sorted-subsample.bam"

regions_df = pd.DataFrame({
    "seq": ["chr1", "chr1"],
    "start": [14000, 20000],
    "end": [20000, 25000],
    "name": ["region_a", "region_b"],
    "strand": ["-", "-"],
})

pld = PileParams(
    input_bam,
    LibFragmentType.Isr,
    regions_df=regions_df,
)

res = pld.generate()
res = res.set_column(4, "down", pa.compute.multiply(res["down"].cast("int64"), -1))

plot_region = res.filter(pa.compute.equal(res["name"], "region_a")).to_pandas()

sns.lineplot(plot_region, x="pos", y="up")
sns.lineplot(plot_region, x="pos", y="down")
plt.title("region_a: chr1:14000-20000 (reverse strand)")
plt.show()
