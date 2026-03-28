rule fetch_fastq:
    output:
        r1="workflow/results/fastq/{sample}_1.fastq.gz",
        r2="workflow/results/fastq/{sample}_2.fastq.gz",
    log:
        "workflow/results/logs/fetch_fastq_{sample}.log",
    params:
        outdir="workflow/results/fastq",
        tmpdir="workflow/results/fastq/tmp_{sample}",
    threads: workflow.cores
    shell:
        """
        (
        mkdir -p {params.tmpdir}
        export VDB_CONFIG={params.tmpdir}/vdb-config
        vdb-config -Q yes
        prefetch {wildcards.sample} -O {params.tmpdir}
        fasterq-dump {wildcards.sample} \
            --outdir {params.outdir} \
            --temp {params.tmpdir} \
            --threads {threads} \
            --split-files
        pigz -p {threads} {params.outdir}/{wildcards.sample}_1.fastq
        pigz -p {threads} {params.outdir}/{wildcards.sample}_2.fastq
        rm -rf {params.tmpdir}
        ) 2> {log}
        """
